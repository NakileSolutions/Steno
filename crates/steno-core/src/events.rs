//! Événements émis par le cœur (l'app les relaie à la WebView et aux notifications).

use crate::model::{Request, Segment};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Levels {
    pub me: f64,
    pub them: f64,
}

/// Appel en cours dans une app de visio surveillée (le plus récent s'il y en a plusieurs).
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct CallState {
    pub active: bool,
    pub app: Option<String>,
    pub app_name: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveState {
    pub state: String,
    pub meeting: Option<crate::model::Meeting>,
    pub levels: Levels,
    pub aside: bool,
    /// Micro coupé dans Sténo : vos paroles ne sont pas transcrites.
    pub mic_muted: bool,
    pub sidecar: String,
    pub last_error: Option<String>,
    pub pending_requests: i64,
    /// Appel détecté (l'écoute ne démarre jamais toute seule).
    pub call: CallState,
}

impl Default for LiveState {
    fn default() -> Self {
        LiveState {
            state: "idle".into(),
            meeting: None,
            levels: Levels { me: -90.0, them: -90.0 },
            aside: false,
            mic_muted: false,
            sidecar: "stopped".into(),
            last_error: None,
            pending_requests: 0,
            call: CallState::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CoreEvent {
    Live(LiveState),
    Segment(Segment),
    /// Phrase retirée de la transcription (jamais dite).
    SegmentDeleted { meeting_id: String, id: i64 },
    Request(Request),
    Report { meeting_id: String, version: i64 },
    MeetingsChanged,
    /// Notification système à afficher (titre, corps, route à ouvrir au clic).
    Notify { title: String, body: String, route: Option<String> },
    /// Une commande vocale vient d'être détectée. `played` : le sidecar a déjà joué le signal
    /// (sinon l'app peut le jouer elle-même).
    CommandDetected { played: bool },
}

impl CoreEvent {
    /// Nom et charge de l'événement envoyé à l'interface (docs/contracts.md §5), ou `None`
    /// pour les événements internes (notifications système, retour sonore).
    pub fn to_wire(&self) -> Option<(&'static str, serde_json::Value)> {
        Some(match self {
            CoreEvent::Live(st) => ("steno://live", serde_json::to_value(st).ok()?),
            CoreEvent::Segment(s) => ("steno://segment", serde_json::to_value(s).ok()?),
            CoreEvent::SegmentDeleted { meeting_id, id } => {
                ("steno://segment_deleted", serde_json::json!({ "meeting_id": meeting_id, "id": id }))
            }
            CoreEvent::Request(r) => ("steno://request", serde_json::to_value(r).ok()?),
            CoreEvent::Report { meeting_id, version } => {
                ("steno://report", serde_json::json!({ "meeting_id": meeting_id, "version": version }))
            }
            CoreEvent::MeetingsChanged => ("steno://meetings", serde_json::Value::Null),
            CoreEvent::Notify { .. } | CoreEvent::CommandDetected { .. } => return None,
        })
    }
}
