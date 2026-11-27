//! Niveaux RMS en dBFS par canal (vumètre). Le sidecar émet `level` au plus 4 fois par seconde.

use std::sync::Mutex;

/// Plancher utilisé quand il n'y a pas de signal.
pub const FLOOR_DB: f64 = -100.0;

pub fn rms(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    (sum / samples.len() as f64).sqrt()
}

pub fn dbfs_of_rms(rms: f64) -> f64 {
    if !rms.is_finite() || rms <= 0.0 {
        return FLOOR_DB;
    }
    (20.0 * rms.log10()).clamp(FLOOR_DB, 0.0)
}

pub fn dbfs(samples: &[f32]) -> f64 {
    dbfs_of_rms(rms(samples))
}

/// Accumule l'énergie par canal entre deux émissions. Sûr entre threads.
#[derive(Default)]
pub struct LevelMeter {
    inner: Mutex<[(f64, usize); 2]>,
}

impl LevelMeter {
    pub fn add(&self, samples: &[f32], channel: crate::protocol::Channel) {
        if samples.is_empty() {
            return;
        }
        let sum: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
        let mut g = self.inner.lock().unwrap();
        let slot = &mut g[channel as usize];
        slot.0 += sum;
        slot.1 += samples.len();
    }

    /// Niveaux depuis le dernier appel (remet les compteurs à zéro).
    pub fn take(&self) -> (f64, f64) {
        let mut g = self.inner.lock().unwrap();
        let level = |(s, n): (f64, usize)| if n == 0 { FLOOR_DB } else { dbfs_of_rms((s / n as f64).sqrt()) };
        let out = (level(g[0]), level(g[1]));
        *g = [(0.0, 0); 2];
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Channel;

    #[test]
    fn niveaux() {
        assert_eq!(dbfs(&[]), FLOOR_DB);
        assert!((dbfs(&[1.0; 10]) - 0.0).abs() < 1e-9);
        assert!((dbfs(&[0.1; 10]) + 20.0).abs() < 1e-6);
        let m = LevelMeter::default();
        m.add(&[0.1; 100], Channel::Them);
        let (me, them) = m.take();
        assert_eq!(me, FLOOR_DB);
        assert!((them + 20.0).abs() < 1e-6);
        assert_eq!(m.take(), (FLOOR_DB, FLOOR_DB));
    }
}
