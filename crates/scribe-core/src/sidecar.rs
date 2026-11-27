//! Protocole et processus du sidecar audio (docs/contracts.md §2).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Turn {
    pub t0: f64,
    pub t1: f64,
    pub speaker: String,
}

/// Messages sidecar → cœur.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SidecarEvent {
    Ready {
        #[serde(default)]
        version: String,
        #[serde(default)]
        asr: String,
    },
    Permissions {
        #[serde(default)]
        microphone: String,
        #[serde(default)]
        system_audio: String,
        #[serde(default)]
        accessibility: String,
    },
    /// Appel détecté ou terminé dans une app surveillée.
    Call {
        active: bool,
        app: String,
        #[serde(default)]
        title: Option<String>,
    },
    /// Ancien format (Teams seulement), toujours accepté.
    TeamsCall {
        active: bool,
        #[serde(default)]
        title: Option<String>,
    },
    Segment {
        ts: String,
        #[serde(default)]
        t0: Option<f64>,
        #[serde(default)]
        t1: Option<f64>,
        channel: String,
        text: String,
        #[serde(default)]
        private: bool,
    },
    Level {
        me: f64,
        them: f64,
    },
    /// Micros et sorties disponibles.
    Devices {
        #[serde(default)]
        inputs: Vec<AudioDevice>,
        #[serde(default)]
        outputs: Vec<AudioDevice>,
    },
    State {
        state: String,
    },
    WakeTest {
        text: String,
    },
    Diarization {
        #[serde(default)]
        meeting_id: String,
        turns: Vec<Turn>,
    },
    Error {
        code: String,
        #[serde(default)]
        message: String,
        #[serde(default)]
        recoverable: bool,
    },
    /// Fin du processus (interne, jamais reçu sur le fil).
    #[serde(skip)]
    Exited { code: Option<i32> },
    #[serde(other)]
    Unknown,
}

/// Périphérique audio (micro ou sortie).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub transport: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AudioDevices {
    pub inputs: Vec<AudioDevice>,
    pub outputs: Vec<AudioDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatchApp {
    pub id: String,
    pub name: String,
    pub bundles: Vec<String>,
}

/// Messages cœur → sidecar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum SidecarCommand {
    /// Apps de visio à surveiller.
    Watch { apps: Vec<WatchApp> },
    Start {
        vocabulary: Vec<String>,
        silence_ms: u32,
        keep_audio: bool,
        audio_dir: String,
        /// Apps dont le son est capté (vide = toutes les apps surveillées).
        #[serde(default)]
        apps: Vec<String>,
        /// UID du micro (`None` = micro par défaut du système).
        #[serde(default)]
        input_device: Option<String>,
    },
    /// Change de micro (pendant la capture ou pour le prochain `start`).
    SetInput { device: Option<String> },
    /// Demande la liste des périphériques.
    Devices,
    /// Joue le signal de détection sur une sortie (`None` = sortie par défaut).
    Play { sound: String, device: Option<String> },
    Stop,
    Aside { on: bool },
    WakeTest { seconds: u32 },
    Diarize { meeting_id: String, audio_dir: String },
    Permissions,
    Shutdown,
}

pub fn parse_line(line: &str) -> Option<SidecarEvent> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    match serde_json::from_str::<SidecarEvent>(line) {
        Ok(e) => Some(e),
        Err(e) => {
            tracing::warn!("ligne du sidecar illisible ({e}) : {line}");
            None
        }
    }
}

/// Processus sidecar lancé.
pub struct SidecarProcess {
    pub commands: mpsc::UnboundedSender<SidecarCommand>,
    child: Child,
}

impl SidecarProcess {
    /// Lance le sidecar. `program` + `args` ; renvoie le processus et le flux d'événements.
    pub fn spawn(program: &str, args: &[String]) -> Result<(SidecarProcess, mpsc::UnboundedReceiver<SidecarEvent>)> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("impossible de lancer le sidecar audio « {program} »"))?;
        let stdout = child.stdout.take().context("stdout du sidecar")?;
        let stderr = child.stderr.take().context("stderr du sidecar")?;
        let mut stdin = child.stdin.take().context("stdin du sidecar")?;
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<SidecarCommand>();

        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                let mut line = serde_json::to_string(&cmd).unwrap_or_default();
                line.push('\n');
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
        });
        let tx = ev_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(ev) = parse_line(&line) {
                    if tx.send(ev).is_err() {
                        break;
                    }
                }
            }
            let _ = tx.send(SidecarEvent::Exited { code: None });
        });
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "scribe_audio", "{line}");
            }
        });
        Ok((SidecarProcess { commands: cmd_tx, child }, ev_rx))
    }

    pub fn send(&self, cmd: SidecarCommand) {
        let _ = self.commands.send(cmd);
    }

    pub async fn shutdown(mut self) {
        self.send(SidecarCommand::Shutdown);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), self.child.wait()).await;
        let _ = self.child.kill().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocole() {
        let e = parse_line(r#"{"type":"segment","ts":"2026-11-26T10:41:12+01:00","t0":668.2,"t1":672.9,"channel":"me","text":"Scribe, cherche…","private":false,"extra":1}"#).unwrap();
        assert!(matches!(e, SidecarEvent::Segment { ref channel, .. } if channel == "me"));
        assert_eq!(parse_line(r#"{"type":"nouveau_type","x":1}"#), Some(SidecarEvent::Unknown));
        assert_eq!(
            parse_line(r#"{"type":"teams_call","active":true,"title":null}"#),
            Some(SidecarEvent::TeamsCall { active: true, title: None })
        );
        let c = serde_json::to_string(&SidecarCommand::Aside { on: true }).unwrap();
        assert_eq!(c, r#"{"cmd":"aside","on":true}"#);
        assert_eq!(
            parse_line(r#"{"type":"call","active":true,"app":"slack","title":"Huddle"}"#),
            Some(SidecarEvent::Call { active: true, app: "slack".into(), title: Some("Huddle".into()) })
        );
        let w = serde_json::to_string(&SidecarCommand::Watch {
            apps: vec![WatchApp { id: "slack".into(), name: "Slack".into(), bundles: vec!["com.tinyspeck.slackmacgap".into()] }],
        })
        .unwrap();
        assert_eq!(w, r#"{"cmd":"watch","apps":[{"id":"slack","name":"Slack","bundles":["com.tinyspeck.slackmacgap"]}]}"#);
        let c = serde_json::to_string(&SidecarCommand::Stop).unwrap();
        assert_eq!(c, r#"{"cmd":"stop"}"#);
    }
}
