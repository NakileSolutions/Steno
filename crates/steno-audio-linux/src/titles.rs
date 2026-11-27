//! Heuristiques pures (testées) : parmi les titres des fenêtres d'une app de visio, lequel
//! désigne l'appel en cours ? Port de `CallTitleHeuristics.swift`.
//!
//! - Teams : fenêtre de réunion, titre nettoyé de « | Microsoft Teams » ;
//! - Slack : fenêtre du huddle (« Huddle… », « Appel… ») avec le nom du canal ;
//! - Zoom : fenêtre « Zoom Meeting » / « Réunion Zoom » (sujet si le titre en contient un) ;
//! - navigateurs : onglet actif « Meet – … » (Google Meet) ;
//! - autres apps : titre de la fenêtre au premier plan, sauf titre générique.
//!
//! `None` plutôt qu'un titre faux : le contrat autorise `"title": null`.

use crate::protocol::WatchedApp;
use crate::text::fold;

#[derive(Debug, Clone, PartialEq)]
pub struct WindowCandidate {
    pub title: String,
    /// Fenêtre principale de l'app (sous X11 : la première fenêtre de premier niveau du processus).
    pub is_main: bool,
    /// Fenêtre active (`_NET_ACTIVE_WINDOW`).
    pub is_focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Teams,
    Slack,
    Zoom,
    Browser,
    Other,
}

const BROWSER_PREFIXES: &[&str] = &[
    "chrome", "google-chrome", "chromium", "com.google.chrome", "org.chromium", "firefox", "org.mozilla.firefox", "msedge", "microsoft-edge",
    "com.microsoft.edge", "brave", "com.brave.browser", "vivaldi", "opera", "librewolf", "zen",
];
const BROWSER_APP_IDS: &[&str] = &["meet", "google-meet", "googlemeet", "browser", "navigateur", "navigateurs"];

pub fn kind(app: &WatchedApp) -> Kind {
    let id = fold(&app.id);
    let prefixes: Vec<String> = app.prefixes.iter().map(|p| p.to_lowercase()).collect();
    let has = |p: &str| prefixes.iter().any(|x| x.starts_with(p));
    if id == "teams" || has("teams") || has("com.github.ismaelmartinez.teams") || has("com.microsoft.teams") {
        return Kind::Teams;
    }
    if id == "slack" || has("slack") || has("com.slack") || has("com.tinyspeck") {
        return Kind::Slack;
    }
    if id == "zoom" || has("zoom") || has("us.zoom") {
        return Kind::Zoom;
    }
    if BROWSER_APP_IDS.contains(&id.as_str()) || prefixes.iter().any(|p| BROWSER_PREFIXES.iter().any(|b| p.starts_with(b))) {
        return Kind::Browser;
    }
    Kind::Other
}

pub fn pick_title(app: &WatchedApp, windows: &[WindowCandidate]) -> Option<String> {
    match kind(app) {
        Kind::Teams => teams_title(windows),
        Kind::Slack => slack_huddle_title(windows),
        Kind::Zoom => zoom_meeting_title(windows),
        Kind::Browser => meet_tab_title(windows),
        Kind::Other => foreground_title(windows, &[app.name.as_str(), app.id.as_str()]),
    }
}

// MARK: Outils

fn words(s: &str) -> Vec<String> {
    fold(s).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}

fn contains_word(s: &str, hints: &[&str]) -> bool {
    let w = words(s);
    hints.iter().any(|h| w.contains(&fold(h)))
}

/// Découpe un titre sur ses séparateurs usuels (entourés d'espaces : « stand-up » reste entier).
pub fn parts(s: &str) -> Vec<String> {
    let mut t = s.to_string();
    for sep in [" | ", "|", " - ", " – ", " — ", " · "] {
        t = t.replace(sep, "\u{1F}");
    }
    t.split('\u{1F}').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect()
}

/// Retire les décorations de début de titre : compteur de non-lus « (3) », puces « • ● * ».
pub fn strip_decorations(s: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^\s*(\(\d+\)|[•●*])\s*").unwrap());
    re.replace(s, "").trim().to_string()
}

fn foreground_first(windows: &[WindowCandidate]) -> Vec<&WindowCandidate> {
    let mut v: Vec<&WindowCandidate> = windows.iter().filter(|w| w.is_focused).collect();
    v.extend(windows.iter().filter(|w| w.is_main && !w.is_focused));
    v.extend(windows.iter().filter(|w| !w.is_main && !w.is_focused));
    v
}

// MARK: Teams

const TEAMS_GENERIC: &[&str] = &[
    "microsoft teams", "teams", "microsoft teams (work or school)", "microsoft teams (travail ou scolaire)", "chat", "conversation", "conversations",
    "activite", "activity", "calendrier", "calendar", "equipes", "teams and channels", "appels", "calls", "fichiers", "files", "onedrive",
    "applications", "apps", "aide", "help", "parametres", "settings", "notifications", "communautes", "communities", "reseau", "network",
    "nouveau message", "new message", "notification", "", "microsoft teams notification", "partage d'ecran", "screen sharing", "teams for linux",
];
const TEAMS_HINTS: &[&str] = &[
    "reunion", "meeting", "daily", "standup", "stand-up", "sync", "point", "rétro", "retro", "sprint", "planning", "demo", "weekly", "hebdo",
    "revue", "review", "comite", "copil", "atelier", "workshop", "kick-off", "kickoff", "1:1", "entretien", "appel avec", "call with", "refinement",
    "grooming", "brief",
];

pub fn teams_title(windows: &[WindowCandidate]) -> Option<String> {
    let mut best: Option<(String, i32)> = None;
    for w in windows {
        let Some(cleaned) = teams_clean(&w.title) else { continue };
        let folded = fold(&cleaned);
        if TEAMS_GENERIC.contains(&folded.as_str()) {
            continue;
        }
        let mut score = 1;
        if TEAMS_HINTS.iter().any(|h| folded.contains(&fold(h))) {
            score += 3;
        }
        if !w.is_main {
            score += 1;
        }
        if score > best.as_ref().map(|b| b.1).unwrap_or(0) {
            best = Some((cleaned, score));
        }
    }
    best.map(|b| b.0)
}

fn teams_clean(raw: &str) -> Option<String> {
    raw.split('|')
        .map(str::trim)
        .map(strip_teams_dash_suffix)
        .filter(|p| !p.is_empty())
        .filter(|p| !fold(p).contains("microsoft teams") && fold(p) != "teams" && !fold(p).contains("teams for linux"))
        .find(|p| !TEAMS_GENERIC.contains(&fold(p).as_str()))
}

fn strip_teams_dash_suffix(s: &str) -> String {
    for sep in [" - ", " – ", " — "] {
        if let Some(i) = s.rfind(sep) {
            if fold(&s[i + sep.len()..]).contains("teams") {
                return s[..i].trim().to_string();
            }
        }
    }
    s.to_string()
}

// MARK: Slack

const SLACK_HINTS: &[&str] = &["huddle", "huddles", "appel", "call", "meeting", "reunion"];

pub fn slack_huddle_title(windows: &[WindowCandidate]) -> Option<String> {
    let mut best: Option<(String, i32)> = None;
    for w in windows {
        let ps: Vec<String> = parts(&strip_decorations(&w.title)).into_iter().filter(|p| fold(p) != "slack").collect();
        let Some(index) = ps.iter().position(|p| contains_word(p, SLACK_HINTS)) else { continue };
        let mut title = ps[index].clone();
        if words(&title).len() <= 1 && index + 1 < ps.len() {
            title = format!("{title} - {}", ps[index + 1]);
        }
        let mut score = 2;
        if title.contains('#') || title.contains('@') {
            score += 1;
        }
        if !w.is_main {
            score += 1;
        }
        if score > best.as_ref().map(|b| b.1).unwrap_or(0) {
            best = Some((title, score));
        }
    }
    best.map(|b| b.0)
}

// MARK: Zoom

const ZOOM_MARKERS: &[&str] = &["Zoom Meeting", "Réunion Zoom", "Zoom Réunion", "Zoom Webinar", "Webinaire Zoom"];

pub fn zoom_meeting_title(windows: &[WindowCandidate]) -> Option<String> {
    let mut best: Option<(String, i32)> = None;
    for w in windows {
        let raw = strip_decorations(&w.title);
        let folded_raw = fold(&raw);
        // Recherche sans casse ni accents : on travaille sur le titre replié (même longueur en
        // caractères pour du texte latin courant).
        let Some((marker, pos)) = ZOOM_MARKERS.iter().find_map(|m| folded_raw.find(&fold(m)).map(|p| (*m, p))) else { continue };
        let marker_len = fold(marker).len();
        let remainder = if folded_raw.len() == raw.len() {
            format!("{} | {}", &raw[..pos], &raw[pos + marker_len..])
        } else {
            folded_raw.replacen(&fold(marker), " | ", 1)
        };
        let topic = parts(&remainder).into_iter().find(|p| !is_zoom_generic(p));
        let score = if topic.is_some() { 3 } else { 2 };
        if score > best.as_ref().map(|b| b.1).unwrap_or(0) {
            best = Some((topic.unwrap_or_else(|| marker.to_string()), score));
        }
    }
    best.map(|b| b.0)
}

fn is_zoom_generic(part: &str) -> bool {
    let f = fold(part);
    if ["", "zoom", "zoom workplace", "zoom.us"].contains(&f.as_str()) {
        return true;
    }
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^\d+\s*-?\s*min(ute)?s?$").unwrap());
    re.is_match(&f) || !f.chars().any(char::is_alphabetic)
}

// MARK: Navigateurs (Google Meet)

const BROWSER_NAMES: &[&str] = &[
    "google chrome", "chrome", "chromium", "safari", "microsoft edge", "edge", "arc", "firefox", "mozilla firefox", "brave", "brave browser", "opera",
    "vivaldi", "librewolf",
];

pub fn meet_tab_title(windows: &[WindowCandidate]) -> Option<String> {
    let mut best: Option<(String, i32)> = None;
    for w in foreground_first(windows) {
        let mut ps = parts(&strip_decorations(&w.title));
        if let Some(cut) = ps.iter().position(|p| BROWSER_NAMES.contains(&fold(p).as_str())) {
            ps.truncate(cut);
        }
        let mut is_meet = false;
        let mut rest: Vec<String> = vec![];
        for p in &ps {
            let f = fold(p);
            if f == "meet" || f == "google meet" {
                is_meet = true;
            } else if f.starts_with("meet:") || f.starts_with("google meet:") {
                is_meet = true;
                if let Some(i) = p.find(':') {
                    let topic = p[i + 1..].trim();
                    if !topic.is_empty() {
                        rest.push(topic.to_string());
                    }
                }
            } else if !is_meet_code(&f) {
                rest.push(p.clone());
            }
        }
        if !is_meet {
            continue;
        }
        let mut score = if rest.is_empty() { 2 } else { 4 };
        if w.is_focused {
            score += 1;
        }
        if score > best.as_ref().map(|b| b.1).unwrap_or(0) {
            best = Some((rest.first().cloned().unwrap_or_else(|| "Google Meet".into()), score));
        }
    }
    best.map(|b| b.0)
}

/// Code de réunion Meet : `abc-defg-hij`.
pub fn is_meet_code(folded: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[a-z]{3}-[a-z]{4}-[a-z]{3}$").unwrap()).is_match(folded)
}

// MARK: Par défaut

pub fn foreground_title(windows: &[WindowCandidate], app_names: &[&str]) -> Option<String> {
    let mut generic: Vec<String> = app_names.iter().map(|n| fold(n)).collect();
    generic.extend(["".into(), "untitled".into(), "sans titre".into()]);
    let mut ordered: Vec<&WindowCandidate> = windows.iter().filter(|w| w.is_focused).collect();
    ordered.extend(windows.iter().filter(|w| w.is_main && !w.is_focused));
    if ordered.is_empty() {
        ordered = windows.iter().collect();
    }
    for w in ordered {
        if let Some(first) = parts(&strip_decorations(&w.title)).into_iter().find(|p| !generic.contains(&fold(p))) {
            return Some(first);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(title: &str, main: bool, focused: bool) -> WindowCandidate {
        WindowCandidate { title: title.into(), is_main: main, is_focused: focused }
    }
    fn app(id: &str, prefixes: &[&str]) -> WatchedApp {
        WatchedApp::new(id, id, prefixes)
    }

    #[test]
    fn familles() {
        assert_eq!(kind(&app("teams", &["teams-for-linux"])), Kind::Teams);
        assert_eq!(kind(&app("x", &["com.slack.Slack"])), Kind::Slack);
        assert_eq!(kind(&app("zoom", &["zoom"])), Kind::Zoom);
        assert_eq!(kind(&app("browser", &["chrome"])), Kind::Browser);
        assert_eq!(kind(&app("discord", &["discord"])), Kind::Other);
    }

    #[test]
    fn titres() {
        assert_eq!(teams_title(&[w("Chat | Microsoft Teams", true, false), w("Daily ORION | Microsoft Teams", false, false)]), Some("Daily ORION".into()));
        assert_eq!(slack_huddle_title(&[w("Huddle : #equipe-flotte - Acme - Slack", false, false)]), Some("Huddle : #equipe-flotte".into()));
        assert_eq!(slack_huddle_title(&[w("#general - Acme - Slack", true, true)]), None);
        assert_eq!(zoom_meeting_title(&[w("Zoom Workplace", true, false), w("Daily – Zoom Meeting", false, true)]), Some("Daily".into()));
        assert_eq!(zoom_meeting_title(&[w("Réunion Zoom", false, true)]), Some("Réunion Zoom".into()));
        assert_eq!(meet_tab_title(&[w("Meet – Point hebdo - Google Chrome - Travail", true, true)]), Some("Point hebdo".into()));
        assert_eq!(meet_tab_title(&[w("Meet - abc-defg-hij - Chromium", true, true)]), Some("Google Meet".into()));
        assert_eq!(meet_tab_title(&[w("Boîte de réception - Mozilla Firefox", true, true)]), None);
        assert_eq!(foreground_title(&[w("(3) Discord | #général", true, true)], &["Discord", "discord"]), Some("#général".into()));
        assert_eq!(foreground_title(&[w("Discord", true, true)], &["Discord"]), None);
    }
}
