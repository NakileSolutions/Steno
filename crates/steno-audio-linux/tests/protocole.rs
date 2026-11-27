//! Test fonctionnel du binaire en mode protocole (sans modèle) : messages de démarrage, erreurs
//! de commande, permissions, périphériques, calendrier et arrêt propre. Fonctionne avec ou sans
//! serveur son (sans serveur, les listes de périphériques sont vides).

#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;

fn next_of(rx: &mpsc::Receiver<Value>, ty: &str, timeout: Duration) -> Value {
    let deadline = Instant::now() + timeout;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(v) if v["type"] == ty => return v,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    panic!("message « {ty} » non reçu");
}

#[test]
fn protocole_de_bout_en_bout() {
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_steno-audio"))
        .env("STENO_HOME", home.path())
        .env("HOME", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let v: Value = serde_json::from_str(&line).expect("ligne JSON valide");
            if tx.send(v).is_err() {
                break;
            }
        }
    });
    let mut stdin = child.stdin.take().unwrap();
    let mut send = |s: &str| {
        stdin.write_all(s.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    };

    let ready = next_of(&rx, "ready", Duration::from_secs(10));
    assert_eq!(ready["asr"], "parakeet-tdt-v3");
    let perms = next_of(&rx, "permissions", Duration::from_secs(5));
    for k in ["microphone", "system_audio", "accessibility", "calendar"] {
        assert!(["granted", "denied", "unknown"].contains(&perms[k].as_str().unwrap()), "{k}");
    }
    let devices = next_of(&rx, "devices", Duration::from_secs(5));
    assert!(devices["inputs"].is_array() && devices["outputs"].is_array());

    send("pas du json");
    assert_eq!(next_of(&rx, "error", Duration::from_secs(5))["code"], "bad_command");
    send(r#"{"cmd":"aside","on":"oui"}"#);
    let e = next_of(&rx, "error", Duration::from_secs(5));
    assert_eq!(e["message"], "Commande \"aside\" : champ \"on\" absent ou invalide.");

    send(r#"{"cmd":"watch","apps":[{"id":"slack","name":"Slack","bundles":["slack"]}]}"#);
    send(r#"{"cmd":"stop"}"#);
    assert_eq!(next_of(&rx, "state", Duration::from_secs(5))["state"], "stopped");
    send(r#"{"cmd":"request_permission","kind":"microphone"}"#);
    next_of(&rx, "permissions", Duration::from_secs(5));
    send(r#"{"cmd":"devices"}"#);
    next_of(&rx, "devices", Duration::from_secs(5));
    // Calendrier : aucune source dans ce dossier personnel vide.
    send(r#"{"cmd":"calendar_events","from":"2026-11-27T08:00:00+01:00","to":"2026-12-04T08:00:00+01:00"}"#);
    assert_eq!(next_of(&rx, "calendar", Duration::from_secs(5))["events"], serde_json::json!([]));
    // Diarisation d'un fichier absent : erreur claire, pas de plantage.
    send(&format!(r#"{{"cmd":"diarize","meeting_id":"m","audio_dir":"{}"}}"#, home.path().display()));
    let e = next_of(&rx, "error", Duration::from_secs(5));
    assert!(e["message"].as_str().unwrap().contains("Fichier audio introuvable"));

    send(r#"{"cmd":"shutdown"}"#);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(st) = child.try_wait().unwrap() {
            assert!(st.success());
            break;
        }
        assert!(Instant::now() < deadline, "le sidecar ne s'arrête pas");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn options_de_la_ligne_de_commande() {
    let out = Command::new(env!("CARGO_BIN_EXE_steno-audio")).arg("--version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), env!("CARGO_PKG_VERSION"));
    let out = Command::new(env!("CARGO_BIN_EXE_steno-audio")).arg("--help").output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("--prefetch-models"));
    let out = Command::new(env!("CARGO_BIN_EXE_steno-audio")).arg("--inconnu").output().unwrap();
    assert_eq!(out.status.code(), Some(64));
    let out = Command::new(env!("CARGO_BIN_EXE_steno-audio")).args(["--list-audio-processes", "--apps", "fax"]).output().unwrap();
    assert_eq!(out.status.code(), Some(64));
}
