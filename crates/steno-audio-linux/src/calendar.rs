//! Calendrier : événements à venir et participants invités (préparation des réunions, noms des
//! intervenants). Lecture seule ; rien n'est écrit. L'équivalent d'EventKit sous Linux :
//!
//! 1. **Evolution Data Server** (GNOME Agenda, Evolution, comptes en ligne Google / Microsoft 365
//!    / Nextcloud de GNOME) : cache local des calendriers distants
//!    (`~/.cache/evolution/calendar/<source>/cache.db`) et calendriers locaux
//!    (`~/.local/share/evolution/calendar/<source>/calendar.ics`), noms lus dans
//!    `~/.config/evolution/sources/<source>.source` ;
//! 2. **fichiers ou adresses `.ics`** listés dans `~/.local/share/Steno/calendars.txt` (une
//!    entrée par ligne : chemin ou adresse https, `#` pour un commentaire) — par exemple
//!    l'adresse iCal secrète d'un agenda Google ou un calendrier Outlook publié. Les adresses
//!    sont relues au plus toutes les 10 minutes.
//!
//! « moi » : participant dont l'adresse est celle d'un compte en ligne de GNOME
//! (`~/.config/goa-1.0/accounts.conf`) ou de `STENO_EMAIL`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};

use crate::ical;
use crate::protocol::{iso8601, CalendarAttendee, CalendarEvent, PermissionValue};

struct Source {
    name: Option<String>,
    text: String,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| home().join(".config"))
}

fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| home().join(".cache"))
}

fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| home().join(".local/share"))
}

/// Liste des `.ics` de l'utilisateur.
pub fn list_file() -> PathBuf {
    match std::env::var("STENO_HOME") {
        Ok(h) if !h.is_empty() => PathBuf::from(h).join("calendars.txt"),
        _ => data_dir().join("Steno").join("calendars.txt"),
    }
}

fn user_entries() -> Vec<String> {
    std::fs::read_to_string(list_file())
        .map(|s| s.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from).collect())
        .unwrap_or_default()
}

/// `DisplayName=…` de la section `[Data Source]` d'un fichier `.source` d'EDS.
pub fn source_display_name(text: &str) -> Option<String> {
    let mut in_main = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_main = l == "[Data Source]";
        } else if in_main {
            if let Some(v) = l.strip_prefix("DisplayName=") {
                return Some(v.trim().to_string()).filter(|v| !v.is_empty());
            }
        }
    }
    None
}

/// Adresses des comptes en ligne de GNOME.
pub fn goa_emails(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| matches!(k.trim(), "Identity" | "PresentationIdentity" | "EmailAddress"))
        .map(|(_, v)| v.trim().to_lowercase())
        .filter(|v| v.contains('@'))
        .collect()
}

fn my_emails() -> Vec<String> {
    let mut v = std::fs::read_to_string(config_dir().join("goa-1.0/accounts.conf")).map(|t| goa_emails(&t)).unwrap_or_default();
    if let Ok(e) = std::env::var("STENO_EMAIL") {
        v.extend(e.split(',').map(|x| x.trim().to_lowercase()).filter(|x| x.contains('@')));
    }
    v
}

fn eds_name(uid: &str) -> Option<String> {
    std::fs::read_to_string(config_dir().join("evolution/sources").join(format!("{uid}.source"))).ok().and_then(|t| source_display_name(&t))
}

/// Composants iCalendar d'un cache SQLite d'EDS (toutes colonnes texte qui en contiennent).
fn read_eds_cache(path: &Path) -> Option<String> {
    let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    let tables: Vec<String> = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'").ok()?.query_map([], |r| r.get(0)).ok()?.filter_map(Result::ok).collect();
    let mut out = String::new();
    for t in tables {
        let Ok(mut stmt) = conn.prepare(&format!("SELECT * FROM \"{}\"", t.replace('"', ""))) else { continue };
        let cols: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
        // ECacheState = 3 : supprimé localement, pas encore synchronisé.
        let state_col = cols.iter().position(|c| c.eq_ignore_ascii_case("ECacheState"));
        let Ok(mut rows) = stmt.query([]) else { continue };
        while let Ok(Some(row)) = rows.next() {
            if let Some(i) = state_col {
                if row.get::<_, i64>(i).ok() == Some(3) {
                    continue;
                }
            }
            for i in 0..cols.len() {
                if let Ok(s) = row.get::<_, String>(i) {
                    if s.contains("BEGIN:VEVENT") {
                        out.push_str(&s);
                        out.push('\n');
                    }
                }
            }
        }
    }
    Some(out)
}

fn eds_sources() -> Vec<Source> {
    let mut out = vec![];
    for (base, file) in [(cache_dir().join("evolution/calendar"), "cache.db"), (data_dir().join("evolution/calendar"), "calendar.ics")] {
        let Ok(rd) = std::fs::read_dir(&base) else { continue };
        for e in rd.flatten() {
            let path = e.path().join(file);
            if !path.is_file() {
                continue;
            }
            let uid = e.file_name().to_string_lossy().to_string();
            let text = if file == "cache.db" { read_eds_cache(&path) } else { std::fs::read_to_string(&path).ok() };
            if let Some(text) = text.filter(|t| t.contains("BEGIN:VEVENT")) {
                let name = eds_name(&uid).or_else(|| (uid == "system").then(|| "Personnel".to_string()));
                out.push(Source { name, text });
            }
        }
    }
    out
}

static URL_CACHE: Mutex<Vec<(String, Instant, String)>> = Mutex::new(vec![]);

fn fetch(url: &str) -> Option<String> {
    {
        let c = URL_CACHE.lock().unwrap();
        if let Some((_, at, text)) = c.iter().find(|(u, _, _)| u == url) {
            if at.elapsed() < Duration::from_secs(600) {
                return Some(text.clone());
            }
        }
    }
    let agent = ureq::AgentBuilder::new().try_proxy_from_env(true).timeout(Duration::from_secs(20)).build();
    // `webcal://` est un alias de https.
    let real = url.strip_prefix("webcal://").map(|r| format!("https://{r}")).unwrap_or_else(|| url.to_string());
    match agent.get(&real).call().ok().and_then(|r| r.into_string().ok()) {
        Some(text) => {
            let mut c = URL_CACHE.lock().unwrap();
            c.retain(|(u, _, _)| u != url);
            c.push((url.to_string(), Instant::now(), text.clone()));
            Some(text)
        }
        None => {
            crate::log::warn("calendrier : adresse .ics injoignable");
            // Dernière version connue, même ancienne.
            URL_CACHE.lock().unwrap().iter().find(|(u, _, _)| u == url).map(|(_, _, t)| t.clone())
        }
    }
}

fn calname(text: &str) -> Option<String> {
    ical::unfold(text).iter().find_map(|l| l.strip_prefix("X-WR-CALNAME:").map(|v| ical::unescape(v).trim().to_string())).filter(|v| !v.is_empty())
}

fn user_sources() -> Vec<Source> {
    user_entries()
        .into_iter()
        .filter_map(|e| {
            let text = if e.starts_with("https://") || e.starts_with("http://") || e.starts_with("webcal://") {
                fetch(&e)?
            } else {
                let p = e.strip_prefix("~/").map(|r| home().join(r)).unwrap_or_else(|| PathBuf::from(&e));
                std::fs::read_to_string(p).ok()?
            };
            Some(Source { name: calname(&text), text })
        })
        .collect()
}

fn sources_available() -> bool {
    if !user_entries().is_empty() {
        return true;
    }
    [(cache_dir().join("evolution/calendar"), "cache.db"), (data_dir().join("evolution/calendar"), "calendar.ics")]
        .iter()
        .any(|(base, f)| std::fs::read_dir(base).map(|rd| rd.flatten().any(|e| e.path().join(f).is_file())).unwrap_or(false))
}

/// `granted` dès qu'une source de calendrier existe (pas d'autorisation à accorder sous Linux).
pub fn status() -> PermissionValue {
    if sources_available() {
        PermissionValue::Granted
    } else {
        PermissionValue::Unknown
    }
}

/// Mise en forme pure (testée) : occurrences triées, hors journées entières, sans doublon, 200 au plus.
pub fn to_events(mut occ: Vec<(Option<String>, ical::Occurrence)>, me: &[String]) -> Vec<CalendarEvent> {
    occ.retain(|(_, o)| !o.all_day);
    occ.sort_by_key(|(_, o)| o.start);
    // Même réunion vue par deux sources (compte en ligne et .ics) : une seule fois.
    let mut seen: Vec<(String, DateTime<Local>)> = vec![];
    let mut out = vec![];
    for (cal, o) in occ {
        let key = (if o.uid.is_empty() { o.summary.clone() } else { o.uid.clone() }, o.start);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let id = if o.uid.is_empty() { format!("{}-{}", o.summary, o.start.timestamp()) } else { format!("{}-{}", o.uid, o.start.timestamp()) };
        out.push(CalendarEvent {
            id,
            title: o.summary.clone(),
            start: iso8601(o.start),
            end: iso8601(o.end),
            all_day: false,
            location: o.location.clone(),
            url: o.url.clone(),
            notes: o.description.clone(),
            calendar: cal,
            attendees: o
                .attendees
                .iter()
                .map(|a| CalendarAttendee {
                    name: a.name.clone(),
                    email: a.email.clone(),
                    status: a.status.clone(),
                    organizer: a.organizer,
                    me: a.email.as_ref().map(|e| me.contains(&e.to_lowercase())).unwrap_or(false),
                })
                .collect(),
        });
        if out.len() >= 200 {
            break;
        }
    }
    out
}

/// Événements entre `from` et `to` (hors journées entières), triés par début.
pub fn events(from: DateTime<Local>, to: DateTime<Local>) -> Vec<CalendarEvent> {
    let mut occ = vec![];
    for s in eds_sources().into_iter().chain(user_sources()) {
        let comps = ical::vevents(&s.text);
        occ.extend(ical::expand(&comps, from, to).into_iter().map(|o| (s.name.clone(), o)));
    }
    to_events(occ, &my_emails())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fichiers_de_configuration() {
        assert_eq!(source_display_name("[Data Source]\nDisplayName=Travail\nEnabled=true\n[Calendar]\nColor=#fff\n"), Some("Travail".into()));
        assert_eq!(goa_emails("[Account account_1]\nProvider=google\nIdentity=Moi@Exemple.fr\nPresentationIdentity=moi@exemple.fr\n"), vec!["moi@exemple.fr", "moi@exemple.fr"]);
        assert_eq!(calname("BEGIN:VCALENDAR\r\nX-WR-CALNAME:Équipe\r\n"), Some("Équipe".into()));
    }

    #[test]
    fn mise_en_forme() {
        let t = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Local);
        let occ = |uid: &str, start: &str, all_day: bool| ical::Occurrence {
            uid: uid.into(),
            summary: uid.into(),
            start: t(start),
            end: t(start) + chrono::Duration::minutes(30),
            all_day,
            location: None,
            url: None,
            description: None,
            attendees: vec![ical::Attendee { name: Some("Moi".into()), email: Some("MOI@exemple.fr".into()), status: "accepted".into(), organizer: false }],
        };
        let v = vec![
            (Some("B".into()), occ("b", "2026-11-27T11:00:00+01:00", false)),
            (Some("A".into()), occ("a", "2026-11-27T09:00:00+01:00", false)),
            (None, occ("a", "2026-11-27T09:00:00+01:00", false)),
            (None, occ("conge", "2026-11-27T00:00:00+01:00", true)),
        ];
        let e = to_events(v, &["moi@exemple.fr".into()]);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].title, "a");
        assert_eq!(e[0].calendar.as_deref(), Some("A"));
        assert!(e[0].attendees[0].me);
    }
}
