//! Assemblage des commandes : on fusionne les énoncés `me` qui se suivent à moins de 2 s
//! d'écart, jusqu'à 1,5 s de silence (PLAN §6.3).

use super::{Detection, Detector};
use crate::model::Intent;
use std::time::{Duration, Instant};

pub const MERGE_GAP_S: f64 = 2.0;
pub const END_SILENCE_S: f64 = 1.5;
/// Attente maximale (horloge murale) d'une suite d'énoncé avant de clore la commande.
pub const FLUSH_AFTER: Duration = Duration::from_millis(3500);

#[derive(Debug, Clone)]
pub struct Utterance {
    pub id: i64,
    pub t0: f64,
    pub t1: f64,
    pub text: String,
}

#[derive(Debug, Clone)]
struct Pending {
    ids: Vec<i64>,
    parts: Vec<String>,
    last_t1: f64,
    last_arrival: Instant,
}

/// Commande complète, prête à être analysée.
#[derive(Debug, Clone)]
pub struct Assembled {
    pub segment_ids: Vec<i64>,
    pub text: String,
    pub detection: Option<Detection>,
}

#[derive(Debug, Default)]
pub struct CommandAssembler {
    pending: Option<Pending>,
    /// Dernier instant de parole vu sur l'un ou l'autre canal (horloge de capture).
    latest_t: f64,
}

impl CommandAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    fn finalize(&mut self, det: &Detector) -> Option<Assembled> {
        let p = self.pending.take()?;
        // Les frontières d'énoncés comptent comme des pauses.
        let text = p.parts.join(", ");
        let detection = det.detect(&text);
        Some(Assembled { segment_ids: p.ids, text, detection })
    }

    /// Un énoncé `me` (non écho, non privé) arrive.
    pub fn push_me(&mut self, det: &Detector, u: &Utterance, now: Instant) -> Vec<Assembled> {
        let mut out = vec![];
        self.latest_t = self.latest_t.max(u.t1);
        if let Some(p) = &mut self.pending {
            if u.t0 - p.last_t1 < MERGE_GAP_S {
                p.ids.push(u.id);
                p.parts.push(u.text.clone());
                p.last_t1 = u.t1;
                p.last_arrival = now;
                return out;
            }
            if let Some(a) = self.finalize(det) {
                out.push(a);
            }
        }
        if det.contains_wake(&u.text) {
            self.pending = Some(Pending {
                ids: vec![u.id],
                parts: vec![u.text.clone()],
                last_t1: u.t1,
                last_arrival: now,
            });
            // « arrête l'écoute », « annule » : rien à attendre, on agit tout de suite.
            if matches!(det.detect(&u.text).map(|d| d.intent), Some(Intent::Stop | Intent::Cancel)) {
                if let Some(a) = self.finalize(det) {
                    out.push(a);
                }
            }
        }
        out
    }

    /// Un énoncé `them` arrive : il fait avancer l'horloge de capture.
    pub fn observe_them(&mut self, det: &Detector, t0: f64) -> Option<Assembled> {
        self.latest_t = self.latest_t.max(t0);
        let p = self.pending.as_ref()?;
        if t0 - p.last_t1 >= END_SILENCE_S + MERGE_GAP_S {
            return self.finalize(det);
        }
        None
    }

    /// À appeler régulièrement (toutes les ~250 ms) : clôt la commande en attente
    /// si aucune suite n'est arrivée.
    pub fn tick(&mut self, det: &Detector, now: Instant) -> Option<Assembled> {
        let p = self.pending.as_ref()?;
        if now.duration_since(p.last_arrival) >= FLUSH_AFTER {
            return self.finalize(det);
        }
        None
    }

    /// Clôt immédiatement (fin de réunion, fin de fichier).
    pub fn flush(&mut self, det: &Detector) -> Option<Assembled> {
        self.finalize(det)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(id: i64, t0: f64, t1: f64, text: &str) -> Utterance {
        Utterance { id, t0, t1, text: text.into() }
    }

    #[test]
    fn fusion_d_enonces() {
        let det = Detector::new(&crate::wake::test_voice());
        let mut a = CommandAssembler::new();
        let now = Instant::now();
        assert!(a.push_me(&det, &u(1, 10.0, 11.0, "Scribe, cherche"), now).is_empty());
        assert!(a.push_me(&det, &u(2, 12.2, 15.0, "les erreurs de ms-booking depuis hier"), now).is_empty());
        let out = a.push_me(&det, &u(3, 30.0, 31.0, "Bon, on passe au sujet suivant"), now);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].segment_ids, vec![1, 2]);
        let d = out[0].detection.as_ref().unwrap();
        assert_eq!(d.intent, Intent::Search);
        assert_eq!(d.query, "les erreurs de ms-booking depuis hier");
        assert!(!a.is_pending());
    }

    #[test]
    fn cloture_par_minuterie() {
        let det = Detector::new(&crate::wake::test_voice());
        let mut a = CommandAssembler::new();
        let t = Instant::now();
        a.push_me(&det, &u(1, 10.0, 12.0, "Scribe, vérifie le ticket APP-1234"), t);
        assert!(a.tick(&det, t + Duration::from_millis(500)).is_none());
        let r = a.tick(&det, t + FLUSH_AFTER).unwrap();
        assert_eq!(r.detection.unwrap().intent, Intent::Check);
    }

    #[test]
    fn arret_immediat() {
        let det = Detector::new(&crate::wake::test_voice());
        let mut a = CommandAssembler::new();
        let out = a.push_me(&det, &u(1, 0.0, 2.0, "Scribe, arrête l'écoute."), Instant::now());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].detection.as_ref().unwrap().intent, Intent::Stop);
        assert!(!a.is_pending());
    }

    #[test]
    fn enonce_ordinaire_ignore() {
        let det = Detector::new(&crate::wake::test_voice());
        let mut a = CommandAssembler::new();
        assert!(a.push_me(&det, &u(1, 0.0, 2.0, "Le script tourne"), Instant::now()).is_empty());
        assert!(!a.is_pending());
    }
}
