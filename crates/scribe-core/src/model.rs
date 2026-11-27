//! Types de données partagés (base, app, CLI). Voir docs/contracts.md §5.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Quick,
    Search,
    Summary,
}

impl Tier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Quick => "quick",
            Tier::Search => "search",
            Tier::Summary => "summary",
        }
    }
    pub fn parse(s: &str) -> Tier {
        match s {
            "quick" => Tier::Quick,
            "summary" => Tier::Summary,
            _ => Tier::Search,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Intent {
    Search,
    Check,
    Note,
    Recap,
    Cancel,
    /// Arrêter l'écoute (« Scribe, arrête l'écoute »).
    Stop,
    /// Question sur l'historique des réunions (écran Recherche, `scribe ask`).
    History,
}

impl Intent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Intent::Search => "search",
            Intent::Check => "check",
            Intent::Note => "note",
            Intent::Recap => "recap",
            Intent::Cancel => "cancel",
            Intent::Stop => "stop",
            Intent::History => "history",
        }
    }
    pub fn parse(s: &str) -> Option<Intent> {
        Some(match s {
            "search" => Intent::Search,
            "check" => Intent::Check,
            "note" => Intent::Note,
            "recap" => Intent::Recap,
            "cancel" => Intent::Cancel,
            "stop" => Intent::Stop,
            "history" => Intent::History,
            _ => return None,
        })
    }
    pub fn all() -> [Intent; 6] {
        [Intent::Search, Intent::Check, Intent::Note, Intent::Recap, Intent::Cancel, Intent::Stop]
    }
    /// Niveau de modèle utilisé pour cette intention.
    pub fn tier(&self) -> Tier {
        match self {
            Intent::Search | Intent::Check | Intent::History => Tier::Search,
            Intent::Note | Intent::Recap | Intent::Cancel | Intent::Stop => Tier::Quick,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Meeting {
    pub id: String,
    pub title: Option<String>,
    pub title_source: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: String,
    pub confidential: bool,
    pub agent_provider: Option<String>,
    pub agent_session_id: Option<String>,
    pub correlation: Option<String>,
    /// App de visio de l'appel (teams, slack…), `None` si l'écoute a démarré sans appel.
    #[serde(default)]
    pub call_app: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSummary {
    #[serde(flatten)]
    pub meeting: Meeting,
    pub correlated: bool,
    pub open_actions: i64,
    pub segment_count: i64,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Segment {
    pub id: i64,
    pub meeting_id: String,
    pub seq: i64,
    pub ts: String,
    pub t0: Option<f64>,
    pub t1: Option<f64>,
    pub channel: String,
    pub speaker: String,
    pub text: String,
    pub private: bool,
    pub is_echo: bool,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Speaker {
    pub label: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: i64,
    pub meeting_id: String,
    pub ts: String,
    pub text: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub meeting_id: Option<String>,
    pub created_at: String,
    pub origin: String,
    pub intent: String,
    pub query: String,
    pub context_seconds: Option<i64>,
    pub tier: String,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error: Option<String>,
    pub summary: Option<String>,
    pub answer_md: Option<String>,
    pub tools_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub meeting_id: String,
    pub markdown: String,
    pub generated_at: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub correlated: bool,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportVersion {
    pub version: i64,
    pub generated_at: String,
    pub correlated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReportRef {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub kind: String,
    #[serde(rename = "ref", default)]
    pub reference: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: i64,
    pub meeting_id: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: i64,
    pub meeting_id: String,
    pub owner: Option<String>,
    pub text: String,
    pub due: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_started_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: i64,
    pub kind: String,
    pub target: Option<String>,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingDetail {
    pub meeting: Meeting,
    pub speakers: Vec<Speaker>,
    pub notes: Vec<Note>,
    pub requests: Vec<Request>,
    pub report: Option<Report>,
    pub report_versions: Vec<ReportVersion>,
    pub refs: Vec<ReportRef>,
    pub decisions: Vec<Decision>,
    pub actions: Vec<Action>,
    pub drafts: Vec<Draft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub meeting_id: String,
    pub title: Option<String>,
    pub started_at: String,
    pub kind: String,
    pub snippet: String,
    pub ts: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub id: i64,
    pub label: String,
    pub path: String,
    pub use_live: bool,
    pub use_correlation: bool,
    pub exclude_globs: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connector {
    pub id: i64,
    pub provider: String,
    pub name: String,
    pub status: String,
    pub detected_at: String,
    pub use_live: bool,
    pub use_correlation: bool,
    pub write_blockable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub meeting_id: Option<String>,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempts: i64,
    pub created_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingFilter {
    pub query: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub correlated: Option<bool>,
    pub open_actions: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionFilter {
    pub status: Option<String>,
    pub owner: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}
