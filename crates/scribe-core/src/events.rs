//! Événements émis par le cœur (l'app les relaie à la WebView et aux notifications).

use crate::model::{Request, Segment};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Levels {
    pub me: f64,
    pub them: f64,
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
