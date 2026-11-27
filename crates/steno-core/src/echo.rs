//! Anti-écho : un segment `me` qui ressemble à plus de 80 % à un segment `them`
//! dans une fenêtre de ±2 s est un écho des haut-parleurs (PLAN §5.1).

use std::collections::VecDeque;

pub const SIMILARITY_THRESHOLD: f64 = 0.8;
pub const WINDOW_S: f64 = 2.0;

#[derive(Debug, Clone)]
struct Recent {
    id: i64,
    t0: f64,
    t1: f64,
    text: String,
}

/// Mémoire courte des derniers segments de chaque canal.
#[derive(Debug, Default)]
pub struct EchoFilter {
    them: VecDeque<Recent>,
    me: VecDeque<Recent>,
}

fn overlaps(a0: f64, a1: f64, b0: f64, b1: f64) -> bool {
    a0 <= b1 + WINDOW_S && b0 <= a1 + WINDOW_S
}

impl EchoFilter {
    pub fn new() -> Self {
        Self::default()
    }

    fn prune(q: &mut VecDeque<Recent>, now_t: f64) {
        while q.front().map(|r| r.t1 + 30.0 < now_t).unwrap_or(false) {
            q.pop_front();
        }
        while q.len() > 50 {
            q.pop_front();
        }
    }

    /// Un segment `me` arrive : renvoie vrai s'il s'agit d'un écho d'un segment `them` récent.
    pub fn check_me(&mut self, id: i64, t0: f64, t1: f64, text: &str) -> bool {
        Self::prune(&mut self.them, t1);
        let echo = self.them.iter().any(|r| {
            overlaps(t0, t1, r.t0, r.t1) && crate::text::similarity(&r.text, text) > SIMILARITY_THRESHOLD
        });
        if !echo {
            self.me.push_back(Recent { id, t0, t1, text: text.to_string() });
            Self::prune(&mut self.me, t1);
        }
        echo
    }

    /// Un segment `them` arrive : renvoie les segments `me` déjà reçus qui en étaient l'écho
    /// (le micro a pu être transcrit avant le canal Teams).
    pub fn add_them(&mut self, t0: f64, t1: f64, text: &str) -> Vec<i64> {
        Self::prune(&mut self.them, t1);
        self.them.push_back(Recent { id: 0, t0, t1, text: text.to_string() });
        let mut echoes = vec![];
        self.me.retain(|r| {
            let e = overlaps(t0, t1, r.t0, r.t1) && crate::text::similarity(&r.text, text) > SIMILARITY_THRESHOLD;
            if e {
                echoes.push(r.id);
            }
            !e
        });
        echoes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_apres_them() {
        let mut f = EchoFilter::new();
        assert!(f.add_them(10.0, 13.0, "Sténo, cherche les erreurs sur ms-booking").is_empty());
        assert!(f.check_me(1, 10.4, 13.5, "Sténo cherche les erreurs sur MS booking"));
        assert!(!f.check_me(2, 10.4, 13.5, "Je pense qu'on devrait livrer jeudi"));
    }

    #[test]
    fn echo_avant_them() {
        let mut f = EchoFilter::new();
        assert!(!f.check_me(7, 20.0, 22.0, "on regarde le ticket APP-1234"));
        assert_eq!(f.add_them(19.8, 22.1, "On regarde le ticket APP-1234."), vec![7]);
    }

    #[test]
    fn hors_fenetre() {
        let mut f = EchoFilter::new();
        f.add_them(10.0, 12.0, "on regarde le ticket");
        assert!(!f.check_me(1, 20.0, 22.0, "on regarde le ticket"));
    }
}
