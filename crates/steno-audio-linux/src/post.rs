//! Fin de réunion : retranscription (`Retranscriber.swift`) et mise en forme de la diarisation
//! (`Diarizer.swift`). Logique pure, testée ; les modèles sont dans `models.rs`.

use crate::protocol::{round, DiarizationTurn, RetranscribeItem, RetranscribedItem};

// MARK: - Retranscription

/// Marge autour des bornes de la phrase (la VAD coupe parfois un peu tôt).
pub const MARGIN: f64 = 0.15;
/// En dessous, la phrase est trop courte : on garde le direct.
pub const MINIMUM_DURATION: f64 = 0.4;

/// Extrait d'un enregistrement 16 kHz, ou `None` si hors du fichier ou trop court.
pub fn slice(samples: &[f32], t0: f64, t1: f64) -> Option<Vec<f32>> {
    let rate = 16_000.0;
    if !(t0.is_finite() && t1.is_finite()) || t1 - t0 < MINIMUM_DURATION {
        return None;
    }
    let a = (((t0 - MARGIN) * rate).floor().max(0.0)) as usize;
    let b = (((t1 + MARGIN) * rate).ceil() as usize).min(samples.len());
    if b <= a || b - a < (MINIMUM_DURATION * rate) as usize {
        return None;
    }
    Some(samples[a..b].to_vec())
}

/// Le nouveau texte remplace-t-il le direct ? Non s'il est vide, sans lettre, part en boucle
/// ou contient bien plus de texte que l'audio n'en permet.
pub fn usable(text: &str, duration: f64) -> bool {
    if !text.chars().any(char::is_alphabetic) {
        return false;
    }
    if has_loop(text) {
        return false;
    }
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    if words.len() >= 12 {
        let distinct: std::collections::HashSet<&&str> = words.iter().collect();
        if (distinct.len() as f64) / (words.len() as f64) < 0.3 {
            return false;
        }
    }
    if duration > 0.0 && text.chars().count() as f64 / duration > 40.0 {
        return false;
    }
    true
}

/// Motif de 1 à 12 caractères répété au moins 7 fois de suite (`(.{1,12}?)\1{6,}`).
fn has_loop(text: &str) -> bool {
    let c: Vec<char> = text.chars().collect();
    for len in 1..=12 {
        if c.len() < len * 7 {
            break;
        }
        for start in 0..=c.len() - len * 7 {
            let pat = &c[start..start + len];
            if (1..7).all(|k| &c[start + k * len..start + (k + 1) * len] == pat) {
                return true;
            }
        }
    }
    false
}

/// Transcrit chaque phrase demandée depuis les fichiers conservés ; les phrases absentes du
/// résultat gardent le texte du direct.
pub fn run(
    items: &[RetranscribeItem],
    audio_dir: &std::path::Path,
    mut transcribe: impl FnMut(&[f32]) -> anyhow::Result<String>,
    correct: impl Fn(&str) -> String,
    load: impl Fn(&std::path::Path) -> anyhow::Result<Vec<f32>>,
) -> Vec<RetranscribedItem> {
    let mut cache: std::collections::HashMap<String, Vec<f32>> = Default::default();
    let mut out = vec![];
    let mut kept = 0;
    let started = std::time::Instant::now();
    for item in items {
        if !cache.contains_key(&item.file) {
            let samples = load(&audio_dir.join(&item.file)).unwrap_or_else(|e| {
                crate::log::warn(&format!("retranscription : {} illisible ({e:#})", item.file));
                vec![]
            });
            cache.insert(item.file.clone(), samples);
        }
        let Some(chunk) = slice(&cache[&item.file], item.t0, item.t1) else {
            kept += 1;
            continue;
        };
        match transcribe(&chunk) {
            Ok(raw) => {
                let raw = raw.trim();
                if !usable(raw, item.t1 - item.t0) {
                    crate::log::info(&format!("retranscription : phrase {} gardée (texte douteux « {} »)", item.id, raw.chars().take(60).collect::<String>()));
                    kept += 1;
                    continue;
                }
                out.push(RetranscribedItem { id: item.id, text: correct(raw) });
            }
            Err(e) => {
                crate::log::warn(&format!("retranscription de la phrase {} : {e:#}", item.id));
                kept += 1;
            }
        }
    }
    crate::log::info(&format!("retranscription : {} phrases réécrites, {kept} gardées, {:.1} s", out.len(), started.elapsed().as_secs_f64()));
    out
}

// MARK: - Diarisation

#[derive(Debug, Clone, PartialEq)]
pub struct RawSpeakerTurn {
    pub speaker: i32,
    pub start: f64,
    pub end: f64,
}

/// Renomme les locuteurs S1, S2… par ordre de première prise de parole et fusionne les tours
/// consécutifs d'un même locuteur séparés de moins de `merge_gap` secondes.
pub fn turns(raw: &[RawSpeakerTurn], merge_gap: f64) -> Vec<DiarizationTurn> {
    let mut sorted: Vec<&RawSpeakerTurn> = raw.iter().filter(|t| t.end > t.start && t.start.is_finite() && t.end.is_finite()).collect();
    sorted.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    let mut labels: Vec<(i32, String)> = vec![];
    let mut result: Vec<DiarizationTurn> = vec![];
    for t in sorted {
        let label = match labels.iter().find(|(id, _)| *id == t.speaker) {
            Some((_, l)) => l.clone(),
            None => {
                let l = format!("S{}", labels.len() + 1);
                labels.push((t.speaker, l.clone()));
                l
            }
        };
        match result.last_mut() {
            Some(last) if last.speaker == label && t.start - last.t1 <= merge_gap => last.t1 = last.t1.max(t.end),
            _ => result.push(DiarizationTurn { t0: t.start, t1: t.end, speaker: label }),
        }
    }
    result.into_iter().map(|t| DiarizationTurn { t0: round(t.t0, 2), t1: round(t.t1, 2), speaker: t.speaker }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraits() {
        let s = vec![0.0; 16_000 * 10];
        assert!(slice(&s, 1.0, 1.2).is_none());
        let x = slice(&s, 1.0, 2.0).unwrap();
        assert_eq!(x.len(), (1.3 * 16_000.0) as usize);
        assert!(slice(&s, 20.0, 22.0).is_none());
        assert_eq!(slice(&s, 0.0, 1.0).unwrap().len(), (1.15 * 16_000.0) as usize);
    }

    #[test]
    fn textes_exploitables() {
        assert!(usable("Ce deuxième sujet concerne la flotte.", 3.0));
        assert!(!usable("1.1.1.1.1.1.1.1.1", 3.0));
        assert!(!usable("...", 1.0));
        assert!(!usable("oui oui oui oui oui oui oui oui oui oui oui oui oui", 30.0));
        assert!(!usable(&"bonjour à tous ".repeat(10), 1.0));
        assert!(usable("Le planning de la semaine prochaine est validé par l'équipe.", 0.0));
    }

    #[test]
    fn tours_de_parole() {
        let raw = vec![
            RawSpeakerTurn { speaker: 3, start: 5.0, end: 6.0 },
            RawSpeakerTurn { speaker: 1, start: 0.0, end: 2.0 },
            RawSpeakerTurn { speaker: 1, start: 2.2, end: 4.0 },
            RawSpeakerTurn { speaker: 1, start: 7.0, end: 7.0 },
            RawSpeakerTurn { speaker: 1, start: 6.5, end: 8.123 },
        ];
        let t = turns(&raw, 0.3);
        assert_eq!(t.len(), 3);
        assert_eq!((t[0].speaker.as_str(), t[0].t0, t[0].t1), ("S1", 0.0, 4.0));
        assert_eq!(t[1].speaker, "S2");
        assert_eq!((t[2].speaker.as_str(), t[2].t1), ("S1", 8.12));
    }

    #[test]
    fn execution_avec_moteur_simule() {
        let dir = tempfile::tempdir().unwrap();
        let items = vec![
            RetranscribeItem { id: 1, file: "them.wav".into(), t0: 0.0, t1: 2.0 },
            RetranscribeItem { id: 2, file: "them.wav".into(), t0: 0.0, t1: 0.1 },
            RetranscribeItem { id: 3, file: "me.wav".into(), t0: 0.0, t1: 2.0 },
        ];
        let out = run(&items, dir.path(), |_| Ok("atlass est prêt".into()), |t| t.replace("atlass", "Atlas"), |p| {
            if p.ends_with("me.wav") {
                anyhow::bail!("absent")
            } else {
                Ok(vec![0.0; 48_000])
            }
        });
        assert_eq!(out, vec![RetranscribedItem { id: 1, text: "Atlas est prêt".into() }]);
    }
}
