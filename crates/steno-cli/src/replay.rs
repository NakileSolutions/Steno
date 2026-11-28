//! `steno replay` : simulateur du sidecar audio. Rejoue un fichier d'exemple au format du
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

/// Périphériques simulés (micro intégré, AirPods, interface USB).
fn fake_devices() -> Value {
    json!({"type": "devices",
        "inputs": [
            {"id": "BuiltInMicrophoneDevice", "name": "Micro du MacBook Pro", "default": true, "transport": "builtin"},
            {"id": "AirPodsPro-UID:input", "name": "AirPods Pro", "default": false, "transport": "bluetooth"},
            {"id": "ScarlettSolo-UID", "name": "Scarlett Solo USB", "default": false, "transport": "usb"}
        ],
        "outputs": [
            {"id": "AirPodsPro-UID:output", "name": "AirPods Pro", "default": true, "transport": "bluetooth"},
            {"id": "BuiltInSpeakerDevice", "name": "Haut-parleurs du MacBook Pro", "default": false, "transport": "builtin"}
        ]})
}

fn is_type(v: &Value, kind: &str) -> bool {
    v.get("type").and_then(|t| t.as_str()) == Some(kind)
}

/// Answer to `calendar_events`: like the real sidecars, only the events that overlap
/// [`from`, `to`) (each bound optional). An event or a bound that does not parse as RFC 3339
/// does not filter anything out.
fn calendar_reply(calendar: &[Value], from: Option<&str>, to: Option<&str>) -> Value {
    let parse = |s: Option<&str>| s.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
    let (from, to) = (parse(from), parse(to));
    let events: Vec<&Value> = calendar
        .iter()
        .filter(|e| {
            let start = parse(e.get("start").and_then(|s| s.as_str()));
            let end = parse(e.get("end").and_then(|s| s.as_str()));
            let ends_after_from = match (end, from) {
                (Some(end), Some(from)) => end > from,
                _ => true,
            };
            let starts_before_to = match (start, to) {
                (Some(start), Some(to)) => start < to,
                _ => true,
            };
            ends_after_from && starts_before_to
        })
        .collect();
    json!({"type": "calendar", "events": events})
}

/// `speed` : facteur d'accélération (0 = aussi vite que possible).
pub fn run(file: &Path, speed: f64, exit_at_end: bool) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("lecture de {}", file.display()))?;
    let lines: Vec<Value> = text.lines().filter(|l| !l.trim().is_empty()).filter_map(|l| serde_json::from_str(l).ok()).collect();
    // `calendar` lines are the simulated calendar: they answer `calendar_events` and are not
    // replayed as events. With none, there is no calendar source (`calendar: unknown`).
    let (calendar_lines, events): (Vec<Value>, Vec<Value>) = lines.into_iter().partition(|v| is_type(v, "calendar"));
    let calendar: Vec<Value> = calendar_lines
        .iter()
        .filter_map(|c| c.get("events").and_then(|e| e.as_array()))
        .flatten()
        .cloned()
        .collect();
    let permissions = json!({"type": "permissions", "microphone": "granted", "system_audio": "granted", "accessibility": "granted",
                             "calendar": if calendar_lines.is_empty() { "unknown" } else { "granted" }});

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
    emit(&permissions);
    emit(&fake_devices());

    // Comme le vrai sidecar : l'appel Teams est annoncé tout de suite, mais rien n'est capté
    // avant la commande `start` ; `stop` met la capture en pause (reprise au `start` suivant).
    let mut aside = false;
    let mut capturing = false;
    let handle_cmd = |c: &Value, aside: &mut bool, capturing: &mut bool| -> bool {
        match c.get("cmd").and_then(|x| x.as_str()).unwrap_or("") {
            "start" => {
                *capturing = true;
                emit(&json!({"type": "state", "state": "listening"}))
            }
            "stop" => {
                *capturing = false;
                emit(&json!({"type": "state", "state": "stopped"}))
            }
            "pause" => {
                *capturing = false;
                emit(&json!({"type": "state", "state": "paused"}))
            }
            "resume" => {
                *capturing = true;
                emit(&json!({"type": "state", "state": "listening"}))
            }
            "aside" => *aside = c.get("on").and_then(|x| x.as_bool()).unwrap_or(false),
            // Liste des apps surveillées : le rejeu annonce les appels du fichier tels quels.
            // Micro coupé : sans effet sur un rejeu (les phrases du fichier sont déjà écrites).
            "watch" | "set_input" | "play" | "mute" => {}
            "devices" => emit(&fake_devices()),
            "wake_test" => emit(&json!({"type": "wake_test", "text": "Sténo, cherche quelque chose."})),
            "diarize" => emit(&json!({"type": "error", "code": "diarize_failed",
                                      "message": "diarisation indisponible en simulation", "recoverable": true})),
            // Rejeu : pas d'audio, la transcription du fichier est gardée telle quelle.
            "retranscribe" => emit(&json!({"type": "retranscription", "meeting_id": c["meeting_id"], "items": []})),
            "calendar_events" => emit(&calendar_reply(
                &calendar,
                c.get("from").and_then(|x| x.as_str()),
                c.get("to").and_then(|x| x.as_str()),
            )),
            "permissions" | "request_permission" => emit(&permissions),
            "shutdown" => return false,
            _ => emit(&json!({"type": "error", "code": "bad_command", "message": c.to_string(), "recoverable": true})),
        }
        true
    };

    let mut idx = 0;
    // Événements d'en-tête (appel Teams, état) avant la première phrase.
    while idx < events.len() && events[idx].get("type").and_then(|t| t.as_str()) != Some("segment") {
        if matches!(events[idx].get("type").and_then(|t| t.as_str()), Some("teams_call" | "call")) {
            emit(&events[idx]);
        }
        idx += 1;
    }
    // Horloge de rejeu : (instant réel, t1 du fichier) au dernier (re)démarrage.
    let mut anchor: Option<(Instant, f64)> = None;
    while idx < events.len() {
        if !capturing {
            anchor = None;
            match rx.recv() {
                Ok(c) => {
                    if !handle_cmd(&c, &mut aside, &mut capturing) {
                        return Ok(());
                    }
                    continue;
                }
                Err(_) => return Ok(()),
            }
        }
        let mut ev = events[idx].clone();
        if let Some(t1) = ev.get("t1").and_then(|t| t.as_f64()) {
            let (at, base) = *anchor.get_or_insert((Instant::now(), t1));
            if speed > 0.0 {
                let due = Duration::from_secs_f64(((t1 - base) / speed).max(0.0));
                let mut interrupted = false;
                loop {
                    let now = at.elapsed();
                    if now >= due {
                        break;
                    }
                    match rx.recv_timeout(due - now) {
                        Ok(c) => {
                            if !handle_cmd(&c, &mut aside, &mut capturing) {
                                return Ok(());
                            }
                            if !capturing {
                                interrupted = true;
                                break;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(_) => return Ok(()),
                    }
                }
                if interrupted {
                    continue;
                }
            }
        }
        while let Ok(c) = rx.try_recv() {
            if !handle_cmd(&c, &mut aside, &mut capturing) {
                return Ok(());
            }
        }
        if !capturing {
            continue;
        }
        if aside && ev.get("channel").and_then(|c| c.as_str()) == Some("me") {
            ev["private"] = json!(true);
        }
        emit(&ev);
        idx += 1;
    }
    if exit_at_end {
        std::thread::sleep(Duration::from_millis(300));
        return Ok(());
    }
    // Reste en vie (comme un vrai sidecar) jusqu'à `shutdown` ou la fin de stdin.
    while let Ok(c) = rx.recv() {
        if !handle_cmd(&c, &mut aside, &mut capturing) {
            break;
        }
    }
    Ok(())
}
