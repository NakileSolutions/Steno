//! Journaux sur stderr uniquement (« [steno-audio] INFO … », recopiés par le cœur dans
//! steno.log) et sortie du protocole sur stdout (une ligne JSON par message).

use std::io::Write;
use std::sync::Mutex;

use crate::protocol::Outgoing;

fn emit(level: &str, msg: &str) {
    let ts = chrono::Local::now().format("%H:%M:%S%.3f");
    let _ = writeln!(std::io::stderr(), "{ts} [steno-audio] {level} {msg}");
}

pub fn debug_enabled() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| std::env::var("STENO_AUDIO_DEBUG").map(|v| v == "1").unwrap_or(false))
}

pub fn debug(msg: &str) {
    if debug_enabled() {
        emit("DEBUG", msg);
    }
}
pub fn info(msg: &str) {
    emit("INFO", msg);
}
pub fn warn(msg: &str) {
    emit("WARN", msg);
}
pub fn error(msg: &str) {
    emit("ERROR", msg);
}

static STDOUT: Mutex<()> = Mutex::new(());

/// Écrit un message du protocole sur stdout (sûr entre threads, une ligne entière à la fois).
pub fn send(msg: &Outgoing) {
    let line = msg.to_line();
    let _g = STDOUT.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = std::io::stdout().lock();
    // Cœur disparu (EPIPE) : on ignore, l'arrêt viendra de la fin de stdin.
    let _ = out.write_all(line.as_bytes());
    let _ = out.flush();
}

/// Limite la répétition d'une même erreur (une par code et par intervalle).
pub struct RateLimiter {
    interval: std::time::Duration,
    last: Mutex<std::collections::HashMap<String, std::time::Instant>>,
}

impl RateLimiter {
    pub fn new(seconds: u64) -> Self {
        RateLimiter { interval: std::time::Duration::from_secs(seconds), last: Default::default() }
    }

    pub fn allow(&self, key: &str) -> bool {
        let mut m = self.last.lock().unwrap();
        let now = std::time::Instant::now();
        match m.get(key) {
            Some(t) if now.duration_since(*t) < self.interval => false,
            _ => {
                m.insert(key.into(), now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn limiteur() {
        let l = super::RateLimiter::new(30);
        assert!(l.allow("tap_failed"));
        assert!(!l.allow("tap_failed"));
        assert!(l.allow("asr_failed"));
    }
}
