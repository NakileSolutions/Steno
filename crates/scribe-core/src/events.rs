//! Événements émis par le cœur (l'app les relaie à la WebView et aux notifications).

use crate::model::{Request, Segment};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Levels {
    pub me: f64,
    pub them: f64,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct TeamsCall {
    pub active: bool,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveState {
    pub state: String,
    pub meeting: Option<crate::model::Meeting>,
    pub levels: Levels,
    pub aside: bool,
    pub sidecar: String,
    pub last_error: Option<String>,
    pub pending_requests: i64,
    /// Appel Teams détecté (l'écoute ne démarre jamais toute seule).
    pub teams_call: TeamsCall,
}

impl Default for LiveState {
    fn default() -> Self {
        LiveState {
            state: "idle".into(),
            meeting: None,
            levels: Levels { me: -90.0, them: -90.0 },
            aside: false,
            sidecar: "stopped".into(),
            last_error: None,
            pending_requests: 0,
            teams_call: TeamsCall::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CoreEvent {
    Live(LiveState),
    Segment(Segment),
    Request(Request),
    Report { meeting_id: String, version: i64 },
    MeetingsChanged,
    /// Notification système à afficher (titre, corps, route à ouvrir au clic).
    Notify { title: String, body: String, route: Option<String> },
    /// Retour sonore : une commande vocale vient d'être détectée.
    CommandDetected,
}

impl CoreEvent {
    /// Nom et charge de l'événement envoyé à l'interface (docs/contracts.md §5), ou `None`
    /// pour les événements internes (notifications système, retour sonore).
    pub fn to_wire(&self) -> Option<(&'static str, serde_json::Value)> {
        Some(match self {
            CoreEvent::Live(st) => ("scribe://live", serde_json::to_value(st).ok()?),
            CoreEvent::Segment(s) => ("scribe://segment", serde_json::to_value(s).ok()?),
            CoreEvent::Request(r) => ("scribe://request", serde_json::to_value(r).ok()?),
            CoreEvent::Report { meeting_id, version } => {
                ("scribe://report", serde_json::json!({ "meeting_id": meeting_id, "version": version }))
            }
            CoreEvent::MeetingsChanged => ("scribe://meetings", serde_json::Value::Null),
            CoreEvent::Notify { .. } | CoreEvent::CommandDetected => return None,
        })
    }
}
