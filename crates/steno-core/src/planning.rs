//! Préparation des réunions : réunions prévues, calendrier macOS, participants invités et
//! préparation guidée par l'agent.
//!
//! - Une **réunion prévue** est une réunion au statut `planned` dont `started_at` est l'heure
//!   prévue. Quand l'écoute démarre dans la fenêtre de cette heure, la réunion prévue est
//!   reprise (contexte, documents, format et participants compris).
//! - Le **calendrier** (EventKit, via le sidecar) propose les réunions à venir et leurs invités.
//! - Les **participants** aident l'agent du compte rendu à nommer les intervenants.
//! - La **préparation guidée** : l'agent pose quelques questions, puis rédige le contexte à
//!   partir des réponses.

use crate::agents::{SessionMode, ToolAccess};
use crate::core::Inner;
use crate::live::LiveCmd;
use crate::model::{Meeting, Tier};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

/// Une réunion prévue est reprise si l'écoute démarre entre 30 min avant et 30 min après
/// l'heure prévue (en minutes).
pub const DUE_BEFORE_MIN: i64 = 30;
pub const DUE_AFTER_MIN: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CalendarAttendee {
    pub name: Option<String>,
    pub email: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub organizer: bool,
    #[serde(default)]
    pub me: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub all_day: bool,
    pub location: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub calendar: Option<String>,
    #[serde(default)]
    pub attendees: Vec<CalendarAttendee>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Participant {
    pub name: String,
    pub email: Option<String>,
    /// `calendar` ou `manual`.
    #[serde(default = "manual")]
    pub source: String,
}

fn manual() -> String {
    "manual".into()
}

/// Événement du calendrier à venir, avec la réunion prévue qui lui correspond éventuellement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpcomingEvent {
    #[serde(flatten)]
    pub event: CalendarEvent,
    pub meeting_id: Option<String>,
}

/// Réunions à venir : réunions prévues, et événements du calendrier (si la permission est
/// accordée). `calendar` : état de la permission (`granted | denied | unknown | unavailable`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Upcoming {
    pub planned: Vec<Meeting>,
    pub events: Vec<UpcomingEvent>,
    pub calendar: String,
}

/// Question de préparation et sa réponse.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrepAnswer {
    pub question: String,
    #[serde(default)]
    pub answer: String,
}

/// Invités d'un événement, sans l'utilisateur lui-même ni ceux qui ont décliné.
pub fn participants_from_event(ev: &CalendarEvent) -> Vec<Participant> {
    let mut out: Vec<Participant> = vec![];
    for a in &ev.attendees {
        if a.me || a.status == "declined" {
            continue;
        }
        let name = a
            .name
            .clone()
            .filter(|n| !n.trim().is_empty() && !n.contains('@'))
            .or_else(|| a.email.as_deref().map(name_from_email))
            .unwrap_or_default();
        if name.is_empty() || out.iter().any(|p| p.name == name) {
            continue;
        }
        out.push(Participant { name, email: a.email.clone(), source: "calendar".into() });
    }
    out
}

/// « marie.dupont@exemple.fr » → « Marie Dupont ».
pub fn name_from_email(email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email);
    local
        .split(['.', '_', '-'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_time(s: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(s).ok()
}

/// Événement en cours (ou qui commence dans les 10 min) le plus proche du titre de l'appel.
pub fn event_for_now<'a>(events: &'a [CalendarEvent], call_title: Option<&str>, now: chrono::DateTime<chrono::Local>) -> Option<&'a CalendarEvent> {
    let current: Vec<&CalendarEvent> = events
        .iter()
        .filter(|e| match (parse_time(&e.start), parse_time(&e.end)) {
            (Some(s), Some(en)) => s - chrono::Duration::minutes(10) <= now && now <= en,
            _ => false,
        })
        .collect();
    if let Some(t) = call_title.map(crate::text::normalize).filter(|t| !t.is_empty()) {
        if let Some(e) = current.iter().find(|e| {
            let et = crate::text::normalize(&e.title);
            !et.is_empty() && (t.contains(&et) || et.contains(&t))
        }) {
            return Some(e);
        }
    }
    // Sinon, celui qui a des invités (une vraie réunion) et commence le plus tard.
    current.into_iter().filter(|e| !e.attendees.is_empty()).max_by_key(|e| e.start.clone())
}

/// Événements du calendrier entre deux instants (via le sidecar).
pub async fn calendar_events(inner: &Inner, from: Option<String>, to: Option<String>) -> Result<Vec<CalendarEvent>> {
    let (tx, rx) = oneshot::channel();
    if !inner.send_live(LiveCmd::Calendar { from, to, reply: tx }) {
        bail!("session en direct indisponible");
    }
    tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .map_err(|_| anyhow!("calendrier : délai dépassé"))?
        .context("calendrier interrompu")?
}

/// État de la permission Calendriers (dernier relevé du sidecar).
pub fn calendar_permission(inner: &Inner) -> String {
    inner
        .db
        .get_runtime("permissions")
        .ok()
        .flatten()
        .and_then(|(v, _)| serde_json::from_str::<serde_json::Value>(&v).ok())
        .and_then(|v| v.get("calendar").and_then(|c| c.as_str()).map(String::from))
        .unwrap_or_else(|| "unknown".into())
}

/// Réunions prévues et événements des `days` prochains jours.
pub async fn upcoming(inner: &Inner, days: i64) -> Result<Upcoming> {
    let planned = inner.db.planned_meetings()?;
    let perm = calendar_permission(inner);
    let mut events = vec![];
    let mut state = perm.clone();
    if perm == "granted" {
        let now = chrono::Local::now();
        let to = now + chrono::Duration::days(days.clamp(1, 31));
        match calendar_events(inner, Some(crate::now_iso()), Some(to.to_rfc3339_opts(chrono::SecondsFormat::Secs, false))).await {
            Ok(evs) => {
                for ev in evs {
                    let meeting_id = inner.db.meeting_for_event(&ev.id)?;
                    events.push(UpcomingEvent { event: ev, meeting_id });
                }
            }
            Err(e) => {
                tracing::warn!("calendrier : {e:#}");
                state = "unavailable".into();
            }
        }
    }
    Ok(Upcoming { planned, events, calendar: state })
}

/// Crée une réunion prévue (éventuellement depuis un événement du calendrier).
pub fn plan_meeting(inner: &Inner, title: &str, scheduled_at: &str, event_id: Option<&str>) -> Result<Meeting> {
    let start = parse_time(scheduled_at).context("date prévue invalide (ISO 8601 attendu)")?;
    if let Some(eid) = event_id {
        if let Some(id) = inner.db.meeting_for_event(eid)? {
            return inner.db.get_meeting(&id)?.context("réunion introuvable");
        }
    }
    let event = event_id.and_then(|eid| inner.calendar_cache.lock().unwrap().iter().find(|e| e.id == eid).cloned());
    let title = Some(title.trim()).filter(|t| !t.is_empty()).map(String::from).or_else(|| event.as_ref().map(|e| e.title.clone()));
    let title = title.unwrap_or_else(|| inner.tr("Réunion prévue", "Planned meeting"));
    let local = start.with_timezone(&chrono::Local);
    let base = format!("{}-{}", local.format("%Y-%m-%d-%H%M"), crate::text::slugify(&title));
    let mut id = base.trim_end_matches('-').to_string();
    let mut n = 2;
    while inner.db.meeting_exists(&id)? {
        id = format!("{base}-{n}");
        n += 1;
    }
    let s = inner.settings();
    let m = Meeting {
        id: id.clone(),
        title: Some(title),
        title_source: Some(if event.is_some() { "calendar".into() } else { "user".into() }),
        started_at: local.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        ended_at: None,
        status: "planned".into(),
        confidential: false,
        agent_provider: None,
        agent_session_id: None,
        correlation: Some(s.correlation.mode.clone()),
        call_app: None,
    };
    inner.db.insert_meeting(&m)?;
    if let Some(ev) = &event {
        inner.db.set_meeting_event(&id, Some(&ev.id))?;
        inner.db.set_participants(&id, &participants_from_event(ev))?;
        // Description de l'invitation : point de départ du contexte.
        if let Some(notes) = ev.notes.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            let intro = inner.tr("Invitation du calendrier :", "Calendar invitation:");
            let (text, _) = crate::context::cap(&format!("{intro}\n{notes}"), crate::context::MAX_FILE_CHARS);
            inner.db.set_meeting_context(&id, &text, None)?;
        }
    } else if let Some(eid) = event_id {
        inner.db.set_meeting_event(&id, Some(eid))?;
    }
    inner.emit(crate::events::CoreEvent::MeetingsChanged);
    Ok(m)
}

/// Rattache à une réunion qui vient de démarrer les invités de l'événement en cours, si elle
/// n'a pas encore de participants.
pub async fn link_calendar(inner: &Inner, meeting_id: &str, call_title: Option<&str>) -> Result<()> {
    if calendar_permission(inner) != "granted" || !inner.db.participants(meeting_id)?.is_empty() {
        return Ok(());
    }
    let now = chrono::Local::now();
    let from = (now - chrono::Duration::hours(2)).to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    let to = (now + chrono::Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    let events = calendar_events(inner, Some(from), Some(to)).await?;
    if let Some(ev) = event_for_now(&events, call_title, now) {
        inner.db.set_meeting_event(meeting_id, Some(&ev.id))?;
        inner.db.set_participants(meeting_id, &participants_from_event(ev))?;
        tracing::info!("réunion {meeting_id} : invités de « {} » rattachés", ev.title);
    }
    Ok(())
}

fn prep_material(inner: &Inner, m: &Meeting) -> Result<String> {
    let c = inner.db.meeting_context(&m.id)?;
    let people = inner.db.participants(&m.id)?;
    let people = if people.is_empty() {
        "(non renseignés)".to_string()
    } else {
        people.iter().map(|p| format!("- {}", p.name)).collect::<Vec<_>>().join("\n")
    };
    let docs: Vec<String> = c.files.iter().map(|f| format!("- {}", f.name)).collect();
    Ok(format!(
        "Réunion : {}\nPrévue : {}\n\n## Participants\n{people}\n\n## Contexte déjà saisi\n{}\n\n## Documents joints\n{}\n",
        m.title.clone().unwrap_or_default(),
        m.started_at,
        if c.text.trim().is_empty() { "(aucun)" } else { c.text.trim() },
        if docs.is_empty() { "(aucun)".to_string() } else { docs.join("\n") },
    ))
}

/// Questions extraites de la réponse de l'agent (liste à puces ou numérotée, ou JSON).
pub fn parse_questions(answer: &str) -> Vec<String> {
    let body = answer.trim();
    if let Some(start) = body.find('[') {
        if let Some(end) = body.rfind(']') {
            if let Ok(v) = serde_json::from_str::<Vec<String>>(&body[start..=end]) {
                return v.into_iter().map(|q| q.trim().to_string()).filter(|q| !q.is_empty()).take(6).collect();
            }
        }
    }
    body.lines()
        .map(str::trim)
        .filter_map(|l| {
            let l = l.trim_start_matches(['-', '*', '•']).trim_start();
            let l = l.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches(['.', ')']).trim();
            (l.ends_with('?') && l.len() > 3).then(|| l.to_string())
        })
        .take(6)
        .collect()
}

async fn ask_agent(inner: &Inner, m: &Meeting, prompt: String) -> Result<String> {
    let res = inner
        .run_agent_with(Some(m), Tier::Quick, ToolAccess::None, SessionMode::None, prompt, 0, None, CancellationToken::new())
        .await?;
    Ok(res.answer)
}

/// Préparation guidée, étape 1 : 3 à 5 questions pour cerner la réunion.
pub async fn prepare_questions(inner: &Inner, meeting_id: &str) -> Result<Vec<String>> {
    let m = inner.db.get_meeting(meeting_id)?.context("réunion introuvable")?;
    let prompt = format!(
        "# Préparation de réunion\n\nAide l'utilisateur à préparer cette réunion. Pose 3 à 5 questions courtes et \
         utiles pour savoir ce qu'il faudra retenir : objectif, décisions attendues, sujets sensibles, chiffres ou \
         règles à surveiller, rôle des participants. Ne pose pas de question dont la réponse figure déjà ci-dessous. \
         N'utilise aucun outil. Réponds uniquement par un tableau JSON de chaînes, sans autre texte.{}\n\n{}",
        if inner.lang() == "en" { " Pose les questions en anglais." } else { "" },
        prep_material(inner, &m)?
    );
    let qs = parse_questions(&ask_agent(inner, &m, prompt).await?);
    if qs.is_empty() {
        bail!("l'agent n'a proposé aucune question");
    }
    Ok(qs)
}

/// Préparation guidée, étape 2 : l'agent rédige le contexte à partir des réponses ; il est
/// ajouté au contexte de la réunion.
pub async fn prepare_apply(inner: &Inner, meeting_id: &str, answers: &[PrepAnswer]) -> Result<crate::context::MeetingContext> {
    let m = inner.db.get_meeting(meeting_id)?.context("réunion introuvable")?;
    let qa: Vec<String> = answers
        .iter()
        .filter(|a| !a.answer.trim().is_empty())
        .map(|a| format!("- **{}** {}", a.question.trim(), a.answer.trim()))
        .collect();
    if qa.is_empty() {
        bail!("aucune réponse");
    }
    let prompt = format!(
        "# Contexte de réunion à rédiger\n\nÀ partir des informations et des réponses ci-dessous, rédige un contexte \
         court (15 lignes au plus) qui servira à l'agent du compte rendu : objectif, sujets, décisions attendues, \
         points à surveiller (chiffres, règles, dates), participants et rôles. Puces Markdown, sans titre de premier \
         niveau, sans phrase d'introduction. N'utilise aucun outil.{}\n\n{}\n## Réponses de l'utilisateur\n{}\n",
        if inner.lang() == "en" { " Rédige en anglais." } else { "" },
        prep_material(inner, &m)?,
        qa.join("\n")
    );
    let brief = ask_agent(inner, &m, prompt).await?;
    let c = inner.db.meeting_context(&m.id)?;
    let heading = inner.tr("Préparation", "Preparation");
    let text = if c.text.trim().is_empty() {
        format!("{heading}\n{}", brief.trim())
    } else {
        format!("{}\n\n{heading}\n{}", c.text.trim(), brief.trim())
    };
    let (text, _) = crate::context::cap(&text, crate::context::MAX_FILE_CHARS);
    inner.db.set_meeting_context(&m.id, &text, c.template.as_deref())?;
    crate::report::context_touched(inner, &m.id)?;
    inner.db.meeting_context(&m.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(title: &str, start: &str, end: &str, people: &[(&str, &str, &str, bool)]) -> CalendarEvent {
        CalendarEvent {
            id: title.into(),
            title: title.into(),
            start: start.into(),
            end: end.into(),
            attendees: people
                .iter()
                .map(|(n, e, s, me)| CalendarAttendee {
                    name: (!n.is_empty()).then(|| n.to_string()),
                    email: (!e.is_empty()).then(|| e.to_string()),
                    status: s.to_string(),
                    organizer: false,
                    me: *me,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn invites_sans_moi_ni_refus() {
        let e = ev(
            "Point",
            "2026-11-27T10:00:00+01:00",
            "2026-11-27T10:30:00+01:00",
            &[
                ("Marie Dupont", "marie@exemple.fr", "accepted", false),
                ("", "paul.martin@exemple.fr", "pending", false),
                ("Moi", "moi@exemple.fr", "accepted", true),
                ("Zoé", "zoe@exemple.fr", "declined", false),
            ],
        );
        let p = participants_from_event(&e);
        assert_eq!(p.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Marie Dupont", "Paul Martin"]);
        assert_eq!(p[0].source, "calendar");
    }

    #[test]
    fn evenement_en_cours() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-27T10:05:00+01:00").unwrap().with_timezone(&chrono::Local);
        let evs = vec![
            ev("Daily ORION", "2026-11-27T10:00:00+01:00", "2026-11-27T10:15:00+01:00", &[("Marie", "", "accepted", false)]),
            ev("Revue Nova", "2026-11-27T10:00:00+01:00", "2026-11-27T11:00:00+01:00", &[("Paul", "", "accepted", false)]),
            ev("Plus tard", "2026-11-27T14:00:00+01:00", "2026-11-27T15:00:00+01:00", &[("Inès", "", "accepted", false)]),
        ];
        assert_eq!(event_for_now(&evs, Some("Revue Nova | Microsoft Teams"), now).unwrap().title, "Revue Nova");
        assert!(event_for_now(&evs, None, now).is_some());
        let late = chrono::DateTime::parse_from_rfc3339("2026-11-27T16:00:00+01:00").unwrap().with_timezone(&chrono::Local);
        assert!(event_for_now(&evs, None, late).is_none());
    }

    #[test]
    fn questions_json_ou_liste() {
        assert_eq!(parse_questions("```json\n[\"Quel est l'objectif ?\", \"Qui décide ?\"]\n```"), ["Quel est l'objectif ?", "Qui décide ?"]);
        assert_eq!(parse_questions("1. Quel budget ?\n2) Quelle date ?\nTexte"), ["Quel budget ?", "Quelle date ?"]);
        assert_eq!(name_from_email("marie.dupont@exemple.fr"), "Marie Dupont");
    }
}
