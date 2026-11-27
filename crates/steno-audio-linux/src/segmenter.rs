//! Détection d'activité vocale de repli (énergie) et découpage en énoncés.
//!
//! Port fidèle de `Segmenter.swift` : `UtteranceSegmenter` est une machine à états pure
//! (testable) qui ferme un énoncé après `silence_ms` de silence ou au bout de 25 s. Un
//! segmenteur indépendant par canal.

use crate::levels::dbfs;

pub const SAMPLE_RATE: usize = 16_000;

/// VAD de repli par énergie, avec plancher de bruit adaptatif. Renvoie 0 ou 1.
pub struct EnergyVad {
    pub margin_db: f64,
    pub absolute_min_db: f64,
    pub noise_floor_db: f64,
}

impl Default for EnergyVad {
    fn default() -> Self {
        EnergyVad { margin_db: 12.0, absolute_min_db: -50.0, noise_floor_db: -60.0 }
    }
}

impl EnergyVad {
    pub fn probability(&mut self, frame: &[f32]) -> f32 {
        let db = dbfs(frame);
        let threshold = self.absolute_min_db.max(self.noise_floor_db + self.margin_db);
        let speech = db > threshold;
        // Plancher : descend vite, remonte lentement (presque pas pendant la parole).
        if db < self.noise_floor_db {
            self.noise_floor_db += 0.2 * (db - self.noise_floor_db);
        } else {
            self.noise_floor_db += (if speech { 0.0005 } else { 0.02 }) * (db - self.noise_floor_db);
        }
        self.noise_floor_db = self.noise_floor_db.clamp(-90.0, -20.0);
        if speech {
            1.0
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    pub samples: Vec<f32>,
    /// Position du premier échantillon (échantillons depuis `start`).
    pub start_sample: usize,
    /// Position juste après le dernier échantillon.
    pub end_sample: usize,
    pub sample_rate: usize,
}

impl Utterance {
    pub fn t0(&self) -> f64 {
        self.start_sample as f64 / self.sample_rate as f64
    }
    pub fn t1(&self) -> f64 {
        self.end_sample as f64 / self.sample_rate as f64
    }
    pub fn duration(&self) -> f64 {
        self.t1() - self.t0()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmenterConfig {
    pub sample_rate: usize,
    /// Silence qui clôt un énoncé.
    pub silence_ms: usize,
    /// Durée maximale d'un énoncé.
    pub max_utterance_seconds: f64,
    /// En dessous, l'énoncé est jeté (clics, toux).
    pub min_speech_ms: usize,
    /// Audio conservé avant le début détecté (attaque des consonnes).
    pub pre_roll_ms: usize,
    /// Silence conservé après la fin.
    pub post_roll_ms: usize,
    /// Hystérésis sur la probabilité de parole.
    pub start_threshold: f32,
    pub continue_threshold: f32,
    /// Fenêtre (fin de tampon) où chercher le point de coupe le plus calme à 25 s.
    pub cut_search_seconds: f64,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        SegmenterConfig {
            sample_rate: SAMPLE_RATE,
            silence_ms: 1000,
            max_utterance_seconds: 25.0,
            min_speech_ms: 200,
            pre_roll_ms: 200,
            post_roll_ms: 150,
            start_threshold: 0.5,
            continue_threshold: 0.35,
            cut_search_seconds: 2.0,
        }
    }
}

/// Machine à états de découpage. Aucune dépendance au temps réel.
pub struct UtteranceSegmenter {
    pub config: SegmenterConfig,
    /// Nombre total d'échantillons consommés (horloge de la session, en échantillons).
    pub position: usize,
    active: bool,
    buffer: Vec<f32>,
    buffer_start: usize,
    speech_samples: usize,
    trailing_silence: usize,
    pre_roll: Vec<f32>,
}

impl UtteranceSegmenter {
    pub fn new(config: SegmenterConfig) -> Self {
        UtteranceSegmenter { config, position: 0, active: false, buffer: vec![], buffer_start: 0, speech_samples: 0, trailing_silence: 0, pre_roll: vec![] }
    }

    #[cfg(test)]
    pub fn is_in_utterance(&self) -> bool {
        self.active
    }

    fn silence_samples(&self) -> usize {
        (self.config.sample_rate * self.config.silence_ms / 1000).max(1)
    }
    fn min_speech_samples(&self) -> usize {
        self.config.sample_rate * self.config.min_speech_ms / 1000
    }
    fn pre_roll_samples(&self) -> usize {
        self.config.sample_rate * self.config.pre_roll_ms / 1000
    }
    fn post_roll_samples(&self) -> usize {
        self.config.sample_rate * self.config.post_roll_ms / 1000
    }
    fn max_samples(&self) -> usize {
        (self.config.max_utterance_seconds * self.config.sample_rate as f64) as usize
    }

    /// Pousse une trame et sa probabilité de parole ; renvoie les énoncés terminés.
    pub fn push(&mut self, frame: &[f32], probability: f32) -> Vec<Utterance> {
        if frame.is_empty() {
            return vec![];
        }
        let frame_start = self.position;
        self.position += frame.len();

        if !self.active {
            if probability >= self.config.start_threshold {
                self.active = true;
                self.buffer_start = frame_start - self.pre_roll.len();
                self.buffer = std::mem::take(&mut self.pre_roll);
                self.buffer.extend_from_slice(frame);
                self.speech_samples = frame.len();
                self.trailing_silence = 0;
            } else {
                self.pre_roll.extend_from_slice(frame);
                let max = self.pre_roll_samples();
                if self.pre_roll.len() > max {
                    let drop = self.pre_roll.len() - max;
                    self.pre_roll.drain(..drop);
                }
            }
            return vec![];
        }

        self.buffer.extend_from_slice(frame);
        if probability >= self.config.continue_threshold {
            self.speech_samples += frame.len();
            self.trailing_silence = 0;
        } else {
            self.trailing_silence += frame.len();
        }

        if self.trailing_silence >= self.silence_samples() {
            return self.close_utterance().into_iter().collect();
        }
        if self.buffer.len() >= self.max_samples() {
            return vec![self.cut_long_utterance()];
        }
        vec![]
    }

    /// Clôt l'énoncé en cours (fin de capture).
    pub fn flush(&mut self) -> Option<Utterance> {
        if !self.active {
            self.pre_roll.clear();
            return None;
        }
        self.close_utterance()
    }

    /// Trou dans le flux : clôt l'énoncé et avance l'horloge.
    pub fn skip(&mut self, samples: usize) -> Option<Utterance> {
        let u = self.flush();
        self.pre_roll.clear();
        self.position += samples;
        u
    }

    fn close_utterance(&mut self) -> Option<Utterance> {
        let drop = self.trailing_silence.saturating_sub(self.post_roll_samples());
        let keep = self.buffer.len().saturating_sub(drop);
        let mut result = None;
        if self.speech_samples >= self.min_speech_samples() && keep > 0 {
            result = Some(Utterance {
                samples: self.buffer[..keep].to_vec(),
                start_sample: self.buffer_start,
                end_sample: self.buffer_start + keep,
                sample_rate: self.config.sample_rate,
            });
        }
        // La fin du tampon (silence) sert de pré-roll au prochain énoncé.
        let pr = self.pre_roll_samples().min(self.buffer.len());
        self.pre_roll = self.buffer[self.buffer.len() - pr..].to_vec();
        self.active = false;
        self.buffer.clear();
        self.speech_samples = 0;
        self.trailing_silence = 0;
        result
    }

    /// Coupe un énoncé trop long au point le plus calme des dernières secondes ; la suite reste
    /// dans le tampon (l'énoncé continue).
    fn cut_long_utterance(&mut self) -> Utterance {
        let cut = self.quietest_cut_point();
        let rest = self.buffer.split_off(cut);
        let head = std::mem::replace(&mut self.buffer, rest);
        let u = Utterance { samples: head, start_sample: self.buffer_start, end_sample: self.buffer_start + cut, sample_rate: self.config.sample_rate };
        self.buffer_start += cut;
        self.speech_samples = self.buffer.len();
        self.trailing_silence = self.trailing_silence.min(self.buffer.len());
        u
    }

    fn quietest_cut_point(&self) -> usize {
        let window = (self.config.sample_rate / 50).max(1); // 20 ms
        let n = self.buffer.len();
        let search = (self.config.cut_search_seconds * self.config.sample_rate as f64) as usize;
        let search_start = window.max(n.saturating_sub(search));
        let mut best = n;
        let mut best_energy = f32::MAX;
        let mut i = search_start;
        while i + window <= n {
            let e: f32 = self.buffer[i..i + window].iter().map(|s| s * s).sum();
            if e < best_energy {
                best_energy = e;
                best = i + window / 2;
            }
            i += window;
        }
        best.clamp(1, n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(seg: &mut UtteranceSegmenter, n: usize, p: f32, v: f32) -> Vec<Utterance> {
        let mut out = vec![];
        for _ in 0..n {
            out.extend(seg.push(&[v; 512], p));
        }
        out
    }

    #[test]
    fn enonce_clos_apres_le_silence() {
        let mut seg = UtteranceSegmenter::new(SegmenterConfig::default());
        assert!(frames(&mut seg, 10, 0.0, 0.0).is_empty());
        assert!(frames(&mut seg, 40, 0.9, 0.3).is_empty());
        assert!(seg.is_in_utterance());
        // 1 s de silence = 31,25 trames de 512.
        let out = frames(&mut seg, 32, 0.0, 0.0);
        assert_eq!(out.len(), 1);
        let u = &out[0];
        // Pré-roll de 200 ms (3200 échantillons) avant la première trame de parole.
        assert_eq!(u.start_sample, 10 * 512 - 3200);
        // Post-roll de 150 ms conservé.
        assert_eq!(u.end_sample, 50 * 512 + 2400);
        assert!(!seg.is_in_utterance());
    }

    #[test]
    fn enonce_trop_court_jete_et_flush() {
        let mut seg = UtteranceSegmenter::new(SegmenterConfig::default());
        frames(&mut seg, 2, 0.9, 0.3); // 64 ms < 200 ms
        assert!(frames(&mut seg, 40, 0.0, 0.0).is_empty());
        frames(&mut seg, 20, 0.9, 0.3);
        assert!(seg.flush().is_some());
        assert!(seg.flush().is_none());
    }

    #[test]
    fn coupe_a_25_secondes_et_trou() {
        let mut seg = UtteranceSegmenter::new(SegmenterConfig::default());
        let out = frames(&mut seg, 800, 0.9, 0.3); // 25,6 s
        assert_eq!(out.len(), 1);
        assert!(out[0].duration() <= 25.0 && out[0].duration() > 22.0);
        assert!(seg.is_in_utterance());
        let pos = seg.position;
        let u = seg.skip(16000).unwrap();
        assert_eq!(u.start_sample, out[0].end_sample);
        assert_eq!(seg.position, pos + 16000);
    }

    #[test]
    fn vad_energie() {
        let mut vad = EnergyVad::default();
        for _ in 0..50 {
            assert_eq!(vad.probability(&[0.0001; 512]), 0.0);
        }
        assert_eq!(vad.probability(&[0.2; 512]), 1.0);
    }
}
