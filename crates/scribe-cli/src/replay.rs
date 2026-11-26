//! `scribe replay` : simulateur du sidecar audio. Rejoue un fichier d'exemple au format du
//! protocole (docs/contracts.md §2) et répond aux commandes du cœur. Permet de tester l'app
//! et le cœur sans Mac ni micro.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn emit(v: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

/// `speed` : facteur d'accélération (0 = aussi vite que possible).
pub fn run(file: &Path, speed: f64, exit_at_end: bool) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("lecture de {}", file.display()))?;
    let events: Vec<Value> = text.lines().filter(|l| !l.trim().is_empty()).filter_map(|l| serde_json::from_str(l).ok()).collect();

    // Commandes du cœur sur un fil dédié.
    let (tx, rx) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
        let _ = tx.send(json!({"cmd": "shutdown"}));
    });

    emit(&json!({"type": "ready", "version": env!("CARGO_PKG_VERSION"), "asr": "replay"}));
    emit(&json!({"type": "permissions", "microphone": "granted", "system_audio": "granted", "accessibility": "granted"}));

    let mut aside = false;
    let handle_cmd = |c: &Value, aside: &mut bool| -> bool {
        match c.get("cmd").and_then(|x| x.as_str()).unwrap_or("") {
            "start" => emit(&json!({"type": "state", "state": "listening"})),
            "stop" => emit(&json!({"type": "state", "state": "stopped"})),
            "aside" => *aside = c.get("on").and_then(|x| x.as_bool()).unwrap_or(false),
            "wake_test" => emit(&json!({"type": "wake_test", "text": "Scribe, cherche quelque chose."})),
            "diarize" => emit(&json!({"type": "error", "code": "diarize_failed",
                                      "message": "diarisation indisponible en simulation", "recoverable": true})),
            "permissions" => emit(&json!({"type": "permissions", "microphone": "granted", "system_audio": "granted", "accessibility": "granted"})),
            "shutdown" => return false,
            _ => emit(&json!({"type": "error", "code": "bad_command", "message": c.to_string(), "recoverable": true})),
        }
        true
    };

    let start = Instant::now();
    let mut first_t: Option<f64> = None;
    for mut ev in events {
        // Cadence : on attend l'instant de fin de l'énoncé (t1), divisé par `speed`.
        if let Some(t1) = ev.get("t1").and_then(|t| t.as_f64()) {
            let base = *first_t.get_or_insert(t1);
            if speed > 0.0 {
                let due = Duration::from_secs_f64(((t1 - base) / speed).max(0.0));
                loop {
                    let now = start.elapsed();
                    if now >= due {
                        break;
                    }
                    match rx.recv_timeout(due - now) {
                        Ok(c) => {
                            if !handle_cmd(&c, &mut aside) {
                                return Ok(());
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(_) => return Ok(()),
                    }
                }
            }
        }
        while let Ok(c) = rx.try_recv() {
            if !handle_cmd(&c, &mut aside) {
                return Ok(());
            }
        }
        if aside && ev.get("channel").and_then(|c| c.as_str()) == Some("me") {
            ev["private"] = json!(true);
        }
        emit(&ev);
        if speed == 0.0 && ev.get("type").and_then(|t| t.as_str()) == Some("teams_call") {
            // Laisser au cœur le temps de créer la réunion.
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if exit_at_end {
        std::thread::sleep(Duration::from_millis(300));
        return Ok(());
    }
    // Reste en vie (comme un vrai sidecar) jusqu'à `shutdown` ou la fin de stdin.
    while let Ok(c) = rx.recv() {
        if !handle_cmd(&c, &mut aside) {
            break;
        }
    }
    Ok(())
}
