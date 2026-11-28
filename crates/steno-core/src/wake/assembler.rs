//! Assemblage des commandes : on fusionne les énoncés `me` qui se suivent à moins de 2 s
//! d'écart, jusqu'à 1,5 s de silence (PLAN §6.3).

use super::{Detection, Detector};
use crate::model::Intent;
use std::time::{Duration, Instant};

pub const MERGE_GAP_S: f64 = 2.0;
pub const END_SILENCE_S: f64 = 1.5;
/// Attente maximale (horloge murale) d'une suite d'énoncé avant de clore la commande.
pub const FLUSH_AFTER: Duration = Duration::from_millis(3500);
/// Shorter wait for « arrête l'écoute » / « annule »: just enough for the `them` channel's copy
/// of a loudspeaker echo to arrive and discard the command (`EchoFilter::add_them`), so a remote
/// participant cannot stop the recording when hardware echo cancellation is off.
pub const STOP_FLUSH_AFTER: Duration = Duration::from_millis(1000);

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
    /// Wait after the last utterance before closing: `FLUSH_AFTER`, or `STOP_FLUSH_AFTER`
    /// while the text so far is a stop or cancel command.
    flush_after: Duration,
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

    fn flush_after(det: &Detector, text: &str) -> Duration {
        match det.detect(text).map(|d| d.intent) {
            Some(Intent::Stop | Intent::Cancel) => STOP_FLUSH_AFTER,
            _ => FLUSH_AFTER,
        }
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
                p.flush_after = Self::flush_after(det, &p.parts.join(", "));
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
                flush_after: Self::flush_after(det, &u.text),
            });
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
        if now.duration_since(p.last_arrival) >= p.flush_after {
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
    fn stop_and_cancel_wait_for_the_echo_guard() {
        let det = Detector::new(&crate::wake::test_voice());
        for (text, intent) in [("Scribe, arrête l'écoute.", Intent::Stop), ("Scribe, annule.", Intent::Cancel)] {
            let mut a = CommandAssembler::new();
            let t = Instant::now();
            assert!(a.push_me(&det, &u(1, 0.0, 2.0, text), t).is_empty(), "{text}: not sent before the echo guard");
            assert!(a.is_pending());
            assert!(a.tick(&det, t + Duration::from_millis(500)).is_none());
            let r = a.tick(&det, t + STOP_FLUSH_AFTER).unwrap();
            assert_eq!(r.detection.unwrap().intent, intent);
            assert!(!a.is_pending());
        }
        assert!(STOP_FLUSH_AFTER < FLUSH_AFTER);
    }

    #[test]
    fn wait_follows_the_command_assembled_so_far() {
        let det = Detector::new(&crate::wake::test_voice());
        let t = Instant::now();
        let mut a = CommandAssembler::new();
        a.push_me(&det, &u(1, 0.0, 1.0, "Scribe, cherche"), t);
        assert!(a.tick(&det, t + STOP_FLUSH_AFTER).is_none(), "a search keeps the normal wait");
        // The wake word alone, then the stop in a second utterance: the shorter wait applies
        // from the second one.
        let mut b = CommandAssembler::new();
        b.push_me(&det, &u(1, 0.0, 1.0, "Scribe,"), t);
        let t2 = t + Duration::from_millis(1500);
        assert!(b.push_me(&det, &u(2, 1.5, 3.0, "arrête l'écoute."), t2).is_empty());
        assert!(b.tick(&det, t2 + Duration::from_millis(500)).is_none());
        assert_eq!(b.tick(&det, t2 + STOP_FLUSH_AFTER).unwrap().detection.unwrap().intent, Intent::Stop);
    }

    #[test]
    fn enonce_ordinaire_ignore() {
        let det = Detector::new(&crate::wake::test_voice());
        let mut a = CommandAssembler::new();
        assert!(a.push_me(&det, &u(1, 0.0, 2.0, "Le script tourne"), Instant::now()).is_empty());
        assert!(!a.is_pending());
    }
}
