//! Protocole JSON lines entre le sidecar et le cœur (docs/contracts.md §2), identique à celui
//! du sidecar Swift (`audio/StenoAudio/Sources/StenoAudioKit/Protocol.swift`).
//!
//! - Une ligne JSON par message, UTF-8, terminée par "\n".
//! - Sidecar → cœur : [`Outgoing`] (champ "type").
//! - Cœur → sidecar : [`Command`] (champ "cmd"). Les champs inconnus sont ignorés ; une commande
//!   inconnue ou illisible produit une erreur `bad_command`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// MARK: - Types partagés

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)] // `denied` : prévu par le contrat, jamais vrai sous Linux (pas d'autorisation).
pub enum PermissionValue {
    Granted,
    Denied,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionsSnapshot {
    pub microphone: PermissionValue,
    pub system_audio: PermissionValue,
    pub accessibility: PermissionValue,
    pub calendar: PermissionValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureState {
    Listening,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Me,
    Them,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Me => "me",
            Channel::Them => "them",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(dead_code)] // `mic_denied`, `system_audio_denied` : propres à macOS.
pub enum ErrorCode {
    #[serde(rename = "mic_denied")]
    MicDenied,
    #[serde(rename = "system_audio_denied")]
    SystemAudioDenied,
    #[serde(rename = "tap_failed")]
    TapFailed,
    #[serde(rename = "device_changed")]
    DeviceChanged,
    /// Micro demandé introuvable (ou débranché) : le micro par défaut est utilisé.
    #[serde(rename = "device_missing")]
    DeviceMissing,
    #[serde(rename = "asr_failed")]
    AsrFailed,
    #[serde(rename = "model_missing")]
    ModelMissing,
    #[serde(rename = "bad_command")]
    BadCommand,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::MicDenied => "mic_denied",
            ErrorCode::SystemAudioDenied => "system_audio_denied",
            ErrorCode::TapFailed => "tap_failed",
            ErrorCode::DeviceChanged => "device_changed",
            ErrorCode::DeviceMissing => "device_missing",
            ErrorCode::AsrFailed => "asr_failed",
            ErrorCode::ModelMissing => "model_missing",
            ErrorCode::BadCommand => "bad_command",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentPayload {
    /// Horodatage ISO 8601 local avec fuseau de la fin de l'énoncé.
    pub ts: String,
    /// Secondes depuis `start`.
    pub t0: f64,
    pub t1: f64,
    pub channel: Channel,
    pub text: String,
    pub private: bool,
}

/// App de visio surveillée (commande `watch`).
///
/// Sous Linux, chaque « préfixe de bundle » est comparé (début, sans casse) au nom de
/// l'exécutable du processus audio, à son identifiant d'app Flatpak/Snap, puis, en dernier
/// recours, au nom d'application annoncé à PulseAudio (voir `calls.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedApp {
    pub id: String,
    pub name: String,
    pub prefixes: Vec<String>,
}

impl WatchedApp {
    pub fn new(id: &str, name: &str, prefixes: &[&str]) -> Self {
        WatchedApp { id: id.into(), name: name.into(), prefixes: prefixes.iter().map(|p| p.to_string()).collect() }
    }

    /// Liste surveillée avant le premier `watch` : Teams seul (docs/contracts.md §2).
    pub fn defaults() -> Vec<WatchedApp> {
        vec![WatchedApp::new("teams", "Microsoft Teams", &["teams-for-linux", "com.github.IsmaelMartinez.teams_for_linux", "teams"])]
    }

    /// Apps connues des modes en ligne de commande (`--apps`).
    pub fn catalog() -> Vec<WatchedApp> {
        let mut v = Self::defaults();
        v.push(WatchedApp::new("slack", "Slack", &["slack", "com.slack.Slack"]));
        v.push(WatchedApp::new("zoom", "Zoom", &["zoom", "us.zoom.Zoom"]));
        v.push(WatchedApp::new("webex", "Webex", &["webex", "CiscoCollabHost", "com.cisco.webex"]));
        v.push(WatchedApp::new("discord", "Discord", &["discord", "com.discordapp.Discord", "vesktop", "dev.vencord.Vesktop"]));
        v.push(WatchedApp::new(
            "meet",
            "Google Meet (navigateur)",
            &["chrome", "google-chrome", "chromium", "com.google.Chrome", "org.chromium.Chromium", "firefox", "org.mozilla.firefox", "msedge", "microsoft-edge", "brave", "com.brave.Browser", "vivaldi", "opera"],
        ));
        v
    }

    /// Longueur du plus long préfixe correspondant (sans casse), `None` si aucun.
    pub fn match_length(&self, candidate: &str) -> Option<usize> {
        let c = candidate.to_lowercase();
        self.prefixes
            .iter()
            .map(|p| p.to_lowercase())
            .filter(|p| !p.is_empty() && c.starts_with(p.as_str()))
            .map(|p| p.chars().count())
            .max()
    }

    /// App à laquelle appartient `candidate` : celle dont le préfixe correspondant est le plus
    /// long ; à égalité, la première de la liste.
    pub fn best_match<'a>(candidate: &str, apps: &'a [WatchedApp]) -> Option<(&'a WatchedApp, usize)> {
        let mut best: Option<(&WatchedApp, usize)> = None;
        for app in apps {
            if let Some(len) = app.match_length(candidate) {
                if len > best.map(|b| b.1).unwrap_or(0) {
                    best = Some((app, len));
                }
            }
        }
        best
    }

    /// `--apps teams,slack` : apps du catalogue, et identifiants inconnus.
    pub fn from_catalog(argument: &str) -> (Vec<WatchedApp>, Vec<String>) {
        let catalog = Self::catalog();
        let (mut apps, mut unknown) = (vec![], vec![]);
        for id in argument.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match catalog.iter().find(|a| a.id == id) {
                Some(a) if !apps.contains(a) => apps.push(a.clone()),
                Some(_) => {}
                None => unknown.push(id.to_string()),
            }
        }
        (apps, unknown)
    }
}

/// Choix d'un périphérique dans une commande : `None` (= `null`) pour le périphérique par défaut.
pub type DeviceSelection = Option<String>;

pub fn selection_label(s: &DeviceSelection) -> String {
    match s {
        Some(uid) => format!("« {uid} »"),
        None => "périphérique par défaut".into(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiarizationTurn {
    pub t0: f64,
    pub t1: f64,
    pub speaker: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RetranscribeItem {
    pub id: i64,
    pub file: String,
    pub t0: f64,
    pub t1: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RetranscribedItem {
    pub id: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    #[serde(rename = "default")]
    pub is_default: bool,
    /// `builtin | usb | bluetooth | virtual | aggregate | other`.
    pub transport: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioDeviceList {
    pub inputs: Vec<AudioDeviceInfo>,
    pub outputs: Vec<AudioDeviceInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalendarAttendee {
    pub name: Option<String>,
    pub email: Option<String>,
    /// `accepted | declined | tentative | pending | unknown`.
    pub status: String,
    pub organizer: bool,
    pub me: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub location: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub calendar: Option<String>,
    pub attendees: Vec<CalendarAttendee>,
}

// MARK: - Sidecar → cœur

#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    Ready { version: String, asr: String },
    Permissions(PermissionsSnapshot),
    /// Un appel commence ou se termine dans une app surveillée.
    Call { active: bool, app: String, title: Option<String> },
    Segment(SegmentPayload),
    Level { me: f64, them: f64 },
    Devices(AudioDeviceList),
    State(CaptureState),
    WakeTest { text: String },
    Diarization { meeting_id: String, turns: Vec<DiarizationTurn> },
    Calendar { events: Vec<CalendarEvent> },
    Retranscription { meeting_id: String, items: Vec<RetranscribedItem> },
    Error { code: ErrorCode, message: String, recoverable: bool },
}

/// Arrondit et neutralise les valeurs non finies.
pub fn round(value: f64, decimals: i32) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let f = 10f64.powi(decimals);
    (value * f).round() / f
}

impl Outgoing {
    pub fn to_json(&self) -> Value {
        match self {
            Outgoing::Ready { version, asr } => json!({"type": "ready", "version": version, "asr": asr}),
            Outgoing::Permissions(p) => json!({
                "type": "permissions",
                "microphone": p.microphone,
                "system_audio": p.system_audio,
                "accessibility": p.accessibility,
                "calendar": p.calendar,
            }),
            // Le contrat veut `"title": null` explicite quand le titre est inconnu.
            Outgoing::Call { active, app, title } => json!({"type": "call", "active": active, "app": app, "title": title}),
            Outgoing::Segment(s) => json!({
                "type": "segment",
                "ts": s.ts,
                "t0": round(s.t0, 2),
                "t1": round(s.t1, 2),
                "channel": s.channel,
                "text": s.text,
                "private": s.private,
            }),
            Outgoing::Level { me, them } => json!({"type": "level", "me": round(*me, 1), "them": round(*them, 1)}),
            Outgoing::Devices(list) => json!({"type": "devices", "inputs": list.inputs, "outputs": list.outputs}),
            Outgoing::State(s) => json!({"type": "state", "state": s}),
            Outgoing::WakeTest { text } => json!({"type": "wake_test", "text": text}),
            Outgoing::Diarization { meeting_id, turns } => json!({"type": "diarization", "meeting_id": meeting_id, "turns": turns}),
            Outgoing::Calendar { events } => json!({"type": "calendar", "events": events}),
            Outgoing::Retranscription { meeting_id, items } => {
                json!({"type": "retranscription", "meeting_id": meeting_id, "items": items})
            }
            Outgoing::Error { code, message, recoverable } => {
                json!({"type": "error", "code": code, "message": message, "recoverable": recoverable})
            }
        }
    }

    pub fn to_line(&self) -> String {
        let mut s = self.to_json().to_string();
        s.push('\n');
        s
    }
}

// MARK: - Cœur → sidecar

#[derive(Debug, Clone, PartialEq)]
pub struct StartOptions {
    pub vocabulary: Vec<String>,
    pub silence_ms: u32,
    pub keep_audio: bool,
    pub audio_dir: Option<String>,
    /// Apps dont le son est capté (canal `them`) ; vide = toutes les apps surveillées.
    pub apps: Vec<String>,
    /// Micro demandé : `None` = champ absent (le choix mémorisé par `set_input` s'applique) ;
    /// `Some(None)` = `"input_device": null` (micro par défaut).
    pub input_device: Option<DeviceSelection>,
    /// Réunion en présentiel : micro seul (ni capture des apps ni annulation d'écho).
    pub room: bool,
}

impl Default for StartOptions {
    fn default() -> Self {
        StartOptions { vocabulary: vec![], silence_ms: 1000, keep_audio: false, audio_dir: None, apps: vec![], input_device: None, room: false }
    }
}

/// Bornes raisonnables pour `silence_ms`.
pub const SILENCE_RANGE: (u32, u32) = (300, 3000);

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Watch(Vec<WatchedApp>),
    Start(StartOptions),
    SetInput(DeviceSelection),
    Devices,
    Play { sound: String, device: DeviceSelection },
    Stop,
    Aside(bool),
    Mute(bool),
    Pause,
    Resume,
    WakeTest { seconds: f64 },
    Diarize { meeting_id: String, audio_dir: String, file: String },
    Retranscribe { meeting_id: String, audio_dir: String, language: String, vocabulary: Vec<String>, items: Vec<RetranscribeItem> },
    Permissions,
    CalendarEvents { from: Option<chrono::DateTime<chrono::FixedOffset>>, to: Option<chrono::DateTime<chrono::FixedOffset>> },
    RequestPermission(String),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    InvalidJson,
    MissingCmd,
    UnknownCommand(String),
    InvalidField { command: String, field: String },
}

impl ParseError {
    /// Message en français destiné à l'utilisateur (champ `message` de l'erreur).
    pub fn message(&self) -> String {
        match self {
            ParseError::InvalidJson => "Commande illisible : JSON invalide.".into(),
            ParseError::MissingCmd => "Commande illisible : champ \"cmd\" absent.".into(),
            ParseError::UnknownCommand(c) => format!("Commande inconnue : \"{c}\"."),
            ParseError::InvalidField { command, field } => format!("Commande \"{command}\" : champ \"{field}\" absent ou invalide."),
        }
    }
}

pub const SOUNDS: [&str; 1] = ["detect"];
pub const PERMISSION_KINDS: [&str; 4] = ["microphone", "system_audio", "accessibility", "calendar"];
const AUDIO_FILES: [&str; 2] = ["them.wav", "me.wav"];

struct Fields<'a> {
    cmd: &'a str,
    obj: &'a Map<String, Value>,
}

impl<'a> Fields<'a> {
    fn bad(&self, field: &str) -> ParseError {
        ParseError::InvalidField { command: self.cmd.into(), field: field.into() }
    }
    fn present(&self, k: &str) -> bool {
        self.obj.contains_key(k)
    }
    /// Champ facultatif : absent ou `null` → None ; mauvais type → erreur.
    fn opt<T>(&self, k: &str, f: impl Fn(&Value) -> Option<T>) -> Result<Option<T>, ParseError> {
        match self.obj.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => f(v).map(Some).ok_or_else(|| self.bad(k)),
        }
    }
    fn string(&self, k: &str) -> Result<Option<String>, ParseError> {
        self.opt(k, |v| v.as_str().map(String::from))
    }
    fn boolean(&self, k: &str) -> Result<Option<bool>, ParseError> {
        self.opt(k, Value::as_bool)
    }
    fn number(&self, k: &str) -> Result<Option<f64>, ParseError> {
        self.opt(k, Value::as_f64)
    }
    fn strings(&self, k: &str) -> Result<Option<Vec<String>>, ParseError> {
        self.opt(k, |v| v.as_array()?.iter().map(|x| x.as_str().map(String::from)).collect())
    }
    /// Champ de périphérique : absent → None ; `null` ou chaîne vide → Some(None) ;
    /// chaîne → Some(Some(uid)) ; autre type → erreur.
    fn selection(&self, k: &str) -> Result<Option<DeviceSelection>, ParseError> {
        match self.obj.get(k) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(s)) => {
                let s = s.trim();
                Ok(Some(if s.is_empty() { None } else { Some(s.to_string()) }))
            }
            Some(_) => Err(self.bad(k)),
        }
    }
}

/// Chaînes nettoyées, non vides, sans doublon (ordre conservé).
pub fn unique_trimmed(values: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for v in values {
        let v = v.trim();
        if !v.is_empty() && !out.iter().any(|o| o == v) {
            out.push(v.to_string());
        }
    }
    out
}

pub fn parse(line: &str) -> Result<Command, ParseError> {
    let value: Value = serde_json::from_str(line).map_err(|_| ParseError::InvalidJson)?;
    let obj = value.as_object().ok_or(ParseError::InvalidJson)?;
    let cmd = match obj.get("cmd") {
        None | Some(Value::Null) => return Err(ParseError::MissingCmd),
        Some(Value::String(s)) => s.as_str(),
        Some(_) => return Err(ParseError::InvalidJson),
    };
    let f = Fields { cmd, obj };
    match cmd {
        "start" => {
            let mut o = StartOptions {
                vocabulary: f.strings("vocabulary")?.unwrap_or_default().iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                ..Default::default()
            };
            if let Some(ms) = f.number("silence_ms")? {
                if ms.is_finite() {
                    o.silence_ms = (ms.round() as i64).clamp(SILENCE_RANGE.0 as i64, SILENCE_RANGE.1 as i64) as u32;
                }
            }
            o.keep_audio = f.boolean("keep_audio")?.unwrap_or(false);
            o.audio_dir = f.string("audio_dir")?.filter(|d| !d.is_empty());
            o.apps = unique_trimmed(&f.strings("apps")?.unwrap_or_default());
            o.input_device = f.selection("input_device")?;
            o.room = f.boolean("room")?.unwrap_or(false);
            Ok(Command::Start(o))
        }
        "set_input" => {
            // `device` est obligatoire (null pour le micro par défaut).
            match f.selection("device")? {
                Some(sel) => Ok(Command::SetInput(sel)),
                None => Err(f.bad("device")),
            }
        }
        "devices" => Ok(Command::Devices),
        "play" => {
            let sound = f.string("sound")?.unwrap_or_else(|| "detect".into()).trim().to_string();
            if !SOUNDS.contains(&sound.as_str()) {
                return Err(f.bad("sound"));
            }
            Ok(Command::Play { sound, device: f.selection("device")?.flatten() })
        }
        "watch" => {
            let entries = match obj.get("apps") {
                Some(Value::Array(a)) => a,
                _ => return Err(f.bad("apps")),
            };
            let mut apps: Vec<WatchedApp> = vec![];
            for e in entries {
                let Some(e) = e.as_object() else { return Err(f.bad("apps")) };
                let id = match e.get("id") {
                    None | Some(Value::Null) => continue,
                    Some(Value::String(s)) => s.trim().to_string(),
                    Some(_) => return Err(f.bad("apps")),
                };
                if id.is_empty() || apps.iter().any(|a| a.id == id) {
                    continue;
                }
                let bundles: Vec<String> = match e.get("bundles") {
                    None | Some(Value::Null) => vec![],
                    Some(Value::Array(b)) => {
                        let mut v = vec![];
                        for x in b {
                            v.push(x.as_str().ok_or_else(|| f.bad("apps"))?.to_string());
                        }
                        v
                    }
                    Some(_) => return Err(f.bad("apps")),
                };
                let prefixes = unique_trimmed(&bundles);
                if prefixes.is_empty() {
                    continue;
                }
                let name = e.get("name").and_then(Value::as_str).map(str::trim).unwrap_or("");
                apps.push(WatchedApp { name: if name.is_empty() { id.clone() } else { name.to_string() }, id, prefixes });
            }
            Ok(Command::Watch(apps))
        }
        "stop" => Ok(Command::Stop),
        "aside" | "mute" => {
            let on = f.boolean("on")?.ok_or_else(|| f.bad("on"))?;
            Ok(if cmd == "aside" { Command::Aside(on) } else { Command::Mute(on) })
        }
        "pause" => Ok(Command::Pause),
        "resume" => Ok(Command::Resume),
        "wake_test" => {
            let mut s = f.number("seconds")?.unwrap_or(8.0);
            if !s.is_finite() {
                s = 8.0;
            }
            Ok(Command::WakeTest { seconds: s.clamp(1.0, 30.0) })
        }
        "diarize" => {
            let meeting_id = f.string("meeting_id")?.filter(|s| !s.is_empty()).ok_or_else(|| f.bad("meeting_id"))?;
            let audio_dir = f.string("audio_dir")?.filter(|s| !s.is_empty()).ok_or_else(|| f.bad("audio_dir"))?;
            let file = f.string("file")?.unwrap_or_else(|| "them.wav".into());
            if !AUDIO_FILES.contains(&file.as_str()) {
                return Err(f.bad("file"));
            }
            Ok(Command::Diarize { meeting_id, audio_dir, file })
        }
        "retranscribe" => {
            let meeting_id = f.string("meeting_id")?.filter(|s| !s.is_empty()).ok_or_else(|| f.bad("meeting_id"))?;
            let audio_dir = f.string("audio_dir")?.filter(|s| !s.is_empty()).ok_or_else(|| f.bad("audio_dir"))?;
            let language = f.string("language")?.filter(|s| !s.is_empty()).ok_or_else(|| f.bad("language"))?;
            let vocabulary = f.strings("vocabulary")?.unwrap_or_default();
            let items: Vec<RetranscribeItem> = match obj.get("items") {
                None | Some(Value::Null) => vec![],
                Some(v) => serde_json::from_value(v.clone()).map_err(|_| f.bad("items"))?,
            };
            if !items.iter().all(|i| AUDIO_FILES.contains(&i.file.as_str())) {
                return Err(f.bad("items"));
            }
            Ok(Command::Retranscribe { meeting_id, audio_dir, language, vocabulary, items })
        }
        "permissions" => Ok(Command::Permissions),
        "calendar_events" => {
            let parse_date = |k: &str| -> Result<Option<chrono::DateTime<chrono::FixedOffset>>, ParseError> {
                match f.string(k)? {
                    None => Ok(None),
                    Some(s) => chrono::DateTime::parse_from_rfc3339(&s).map(Some).map_err(|_| f.bad(k)),
                }
            };
            Ok(Command::CalendarEvents { from: parse_date("from")?, to: parse_date("to")? })
        }
        "request_permission" => {
            let kind = f.string("kind")?.filter(|k| PERMISSION_KINDS.contains(&k.as_str())).ok_or_else(|| f.bad("kind"))?;
            Ok(Command::RequestPermission(kind))
        }
        "shutdown" => Ok(Command::Shutdown),
        other => {
            let _ = f.present("cmd");
            Err(ParseError::UnknownCommand(other.into()))
        }
    }
}

/// `2026-11-26T10:41:12+01:00` : ISO 8601, heure locale, décalage avec deux-points (jamais « Z »).
pub fn iso8601(t: chrono::DateTime<chrono::Local>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_sortants_au_format_du_contrat() {
        let s = Outgoing::Segment(SegmentPayload { ts: "2026-11-26T10:41:12+01:00".into(), t0: 668.234, t1: 672.9, channel: Channel::Me, text: "Sténo".into(), private: false });
        let v = s.to_json();
        assert_eq!(v["type"], "segment");
        assert_eq!(v["t0"], 668.23);
        assert_eq!(v["channel"], "me");
        assert_eq!(v["private"], false);
        let c = Outgoing::Call { active: true, app: "slack".into(), title: None }.to_json();
        assert!(c.as_object().unwrap().contains_key("title"));
        assert!(c["title"].is_null());
        let l = Outgoing::Level { me: f64::NEG_INFINITY, them: -24.14 }.to_json();
        assert_eq!(l["me"], 0.0);
        assert_eq!(l["them"], -24.1);
        let e = Outgoing::Error { code: ErrorCode::TapFailed, message: "x".into(), recoverable: true }.to_json();
        assert_eq!(e["code"], "tap_failed");
        let d = Outgoing::Devices(AudioDeviceList { inputs: vec![AudioDeviceInfo { id: "a".into(), name: "A".into(), is_default: true, transport: "usb".into() }], outputs: vec![] }).to_json();
        assert_eq!(d["inputs"][0]["default"], true);
        let line = Outgoing::State(CaptureState::Listening).to_line();
        assert!(line.ends_with('\n') && !line.trim_end().contains('\n'));
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), json!({"type": "state", "state": "listening"}));
    }

    #[test]
    fn start_complet_et_bornes() {
        let c = parse(r#"{"cmd":"start","vocabulary":[" Sténo ",""],"silence_ms":50,"keep_audio":true,"audio_dir":"/tmp/x","apps":["slack","slack"," "],"input_device":null,"room":true,"inconnu":1}"#).unwrap();
        let Command::Start(o) = c else { panic!() };
        assert_eq!(o.vocabulary, vec!["Sténo"]);
        assert_eq!(o.silence_ms, 300);
        assert!(o.keep_audio && o.room);
        assert_eq!(o.apps, vec!["slack"]);
        assert_eq!(o.input_device, Some(None));
        let Command::Start(o) = parse(r#"{"cmd":"start"}"#).unwrap() else { panic!() };
        assert_eq!(o.input_device, None);
        assert_eq!(o.silence_ms, 1000);
        let Command::Start(o) = parse(r#"{"cmd":"start","input_device":"alsa_input.usb"}"#).unwrap() else { panic!() };
        assert_eq!(o.input_device, Some(Some("alsa_input.usb".into())));
    }

    #[test]
    fn erreurs_de_commande() {
        assert_eq!(parse("pas du json"), Err(ParseError::InvalidJson));
        assert_eq!(parse(r#"{"x":1}"#), Err(ParseError::MissingCmd));
        assert_eq!(parse(r#"{"cmd":"vole"}"#), Err(ParseError::UnknownCommand("vole".into())));
        assert_eq!(parse(r#"{"cmd":"aside","on":"oui"}"#).unwrap_err().message(), "Commande \"aside\" : champ \"on\" absent ou invalide.");
        assert!(parse(r#"{"cmd":"set_input"}"#).is_err());
        assert_eq!(parse(r#"{"cmd":"set_input","device":null}"#), Ok(Command::SetInput(None)));
        assert!(parse(r#"{"cmd":"play","sound":"boum"}"#).is_err());
        assert_eq!(parse(r#"{"cmd":"play"}"#), Ok(Command::Play { sound: "detect".into(), device: None }));
        assert!(parse(r#"{"cmd":"diarize","meeting_id":"m","audio_dir":"/a","file":"x.wav"}"#).is_err());
        assert!(parse(r#"{"cmd":"request_permission","kind":"camera"}"#).is_err());
        assert!(parse(r#"{"cmd":"calendar_events","from":"hier"}"#).is_err());
        assert!(matches!(parse(r#"{"cmd":"calendar_events","from":"2026-11-27T08:00:00+01:00"}"#), Ok(Command::CalendarEvents { from: Some(_), to: None })));
        assert_eq!(parse(r#"{"cmd":"wake_test","seconds":90}"#), Ok(Command::WakeTest { seconds: 30.0 }));
    }

    #[test]
    fn watch_filtre_les_entrees() {
        let Command::Watch(apps) = parse(r#"{"cmd":"watch","apps":[{"id":"teams","name":"Microsoft Teams","bundles":["teams-for-linux"]},{"id":"teams","bundles":["x"]},{"id":"vide","bundles":[]},{"bundles":["y"]},{"id":"slack","bundles":["slack"," slack "]}]}"#).unwrap() else { panic!() };
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[1].name, "slack");
        assert_eq!(apps[1].prefixes, vec!["slack"]);
        assert!(parse(r#"{"cmd":"watch"}"#).is_err());
    }

    #[test]
    fn retranscribe_items() {
        let c = parse(r#"{"cmd":"retranscribe","meeting_id":"m","audio_dir":"/a","language":"fr","vocabulary":["Atlas"],"items":[{"id":42,"file":"them.wav","t0":12.0,"t1":18.5}]}"#).unwrap();
        let Command::Retranscribe { items, .. } = c else { panic!() };
        assert_eq!(items[0].id, 42);
        assert!(parse(r#"{"cmd":"retranscribe","meeting_id":"m","audio_dir":"/a","language":"fr","items":[{"id":1,"file":"autre.wav","t0":0,"t1":1}]}"#).is_err());
    }

    #[test]
    fn correspondance_des_apps() {
        let apps = WatchedApp::catalog();
        assert_eq!(WatchedApp::best_match("teams-for-linux", &apps).unwrap().0.id, "teams");
        assert_eq!(WatchedApp::best_match("ZoomWebviewHost", &apps).unwrap().0.id, "zoom");
        assert_eq!(WatchedApp::best_match("com.slack.Slack", &apps).unwrap().0.id, "slack");
        assert!(WatchedApp::best_match("pipewire", &apps).is_none());
        let (a, u) = WatchedApp::from_catalog("teams,slack,inconnue");
        assert_eq!(a.len(), 2);
        assert_eq!(u, vec!["inconnue"]);
    }

    #[test]
    fn horodatage_sans_z() {
        let t = chrono::Local::now();
        let s = iso8601(t);
        assert!(!s.ends_with('Z'));
        assert_eq!(&s[s.len() - 3..s.len() - 2], ":");
    }
}
