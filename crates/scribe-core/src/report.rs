//! Compte rendu de fin de réunion : génération et analyse de la sortie de l'agent (PLAN §7.1).

use crate::agents::{SessionMode, ToolAccess};
use crate::core::Inner;
use crate::events::CoreEvent;
use crate::model::{Meeting, Tier};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ActionJson {
    pub owner: Option<String>,
    pub text: String,
    pub due: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DraftJson {
    pub kind: String,
    pub target: Option<String>,
    pub body: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ReportJson {
    pub decisions: Vec<String>,
    pub actions: Vec<ActionJson>,
    pub speakers: HashMap<String, String>,
    pub drafts: Vec<DraftJson>,
    pub refs: Vec<crate::model::ReportRef>,
}

/// Sépare le dernier bloc ```json``` du markdown. Renvoie (markdown sans le bloc, JSON).
pub fn split_json(answer: &str) -> (String, Option<serde_json::Value>) {
    let Some(start) = answer.rfind("```json") else { return (answer.trim().to_string(), None) };
    let after = &answer[start + 7..];
    let Some(end) = after.find("```") else { return (answer.trim().to_string(), None) };
    let json_text = after[..end].trim();
    match serde_json::from_str::<serde_json::Value>(json_text) {
        Ok(v) => {
            let mut md = answer[..start].to_string();
            md.push_str(&after[end + 3..]);
            (md.trim().to_string(), Some(v))
        }
        Err(_) => (answer.trim().to_string(), None),
    }
}

/// Contenu d'une section `## <titre…>` (jusqu'à la section suivante de même niveau ou plus haut).
pub fn section(md: &str, heading_prefix: &str) -> Option<String> {
    let mut out = String::new();
    let mut inside = false;
    for line in md.lines() {
        let t = line.trim_start();
        if t.starts_with("## ") || t.starts_with("# ") {
            if inside {
                break;
            }
            let h = t.trim_start_matches('#').trim();
            if crate::text::normalize(h).starts_with(&crate::text::normalize(heading_prefix)) {
                inside = true;
                continue;
            }
        } else if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    inside.then(|| out.trim().to_string())
}

/// Retire une section `## <titre…>` du markdown.
pub fn remove_section(md: &str, heading_prefix: &str) -> String {
    let mut out = vec![];
    let mut skipping = false;
    for line in md.lines() {
        let t = line.trim_start();
        if t.starts_with("## ") || t.starts_with("# ") {
            let h = t.trim_start_matches('#').trim();
            skipping = t.starts_with("## ")
                && crate::text::normalize(h).starts_with(&crate::text::normalize(heading_prefix));
            if skipping {
                continue;
            }
        }
        if !skipping {
            out.push(line);
        }
    }
    out.join("\n").trim().to_string()
}

fn bullets(body: &str) -> Vec<String> {
    body.lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with("- ") || l.starts_with("* ") || l.starts_with("• "))
        .map(|l| l[2..].trim().to_string())
        .filter(|l| !l.is_empty() && !matches!(l.as_str(), "Aucune" | "Aucune." | "(aucune)" | "—" | "-"))
        .collect()
}

fn strip_md(s: &str) -> String {
    s.replace("**", "").replace('`', "").trim().to_string()
}

/// Repli : décisions et actions lues dans les sections markdown.
pub fn parse_markdown(md: &str) -> ReportJson {
    let mut r = ReportJson::default();
    if let Some(d) = section(md, "Décisions") {
        r.decisions = bullets(&d).into_iter().map(|b| strip_md(&b)).collect();
    }
    if let Some(a) = section(md, "Actions") {
        for b in bullets(&a) {
            let parts: Vec<String> = b.split(" — ").map(strip_md).collect();
            let (owner, text, due) = match parts.len() {
                0 | 1 => (None, strip_md(&b), None),
                2 => (Some(parts[0].clone()), parts[1].clone(), None),
                _ => (Some(parts[0].clone()), parts[1].clone(), Some(parts[2].clone())),
            };
            let due = due.filter(|d| !d.is_empty() && d != "—" && d != "-");
            r.actions.push(ActionJson { owner: owner.filter(|o| !o.is_empty()), text, due });
        }
    }
    r
}

/// Extrait le résumé de 2 lignes (`TL;DR: …`), ou les premières lignes.
pub fn extract_summary(answer: &str) -> String {
    let lines: Vec<&str> = answer.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    for (i, l) in lines.iter().enumerate() {
        let lc = l.trim_start_matches(['*', '#', ' ']).to_string();
        let low = lc.to_lowercase();
        if low.starts_with("tl;dr") || low.starts_with("tldr") {
            let body = lc.splitn(2, ':').nth(1).unwrap_or("").replace("**", "").trim().to_string();
            let mut s = body;
            if s.is_empty() {
                s = lines.get(i + 1).map(|x| x.to_string()).unwrap_or_default();
            }
            return crate::text::truncate(&s, 220);
        }
    }
    let first: Vec<&str> = lines.iter().filter(|l| !l.starts_with('#')).take(2).cloned().collect();
    crate::text::truncate(&strip_md(&first.join(" ")), 220)
}

fn ensure_title(md: &str, meeting: &Meeting, date: &str) -> String {
    if md.trim_start().starts_with("# ") {
        return md.to_string();
    }
    format!("# {} — {}\n\n{}", meeting.title.clone().unwrap_or_else(|| "Réunion".into()), date, md)
}

pub fn meeting_date(m: &Meeting) -> String {
    chrono::DateTime::parse_from_rfc3339(&m.started_at)
        .map(|d| d.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_else(|_| m.started_at.clone())
}

/// Génère (ou régénère) le compte rendu d'une réunion.
pub async fn generate(inner: &Inner, meeting_id: &str) -> Result<()> {
    let meeting = inner.db.get_meeting(meeting_id)?.context("réunion introuvable")?;
    if meeting.confidential {
        inner.db.set_meeting_status(meeting_id, "done")?;
        return Ok(());
    }
    inner.db.set_meeting_status(meeting_id, "processing")?;
    inner.emit(CoreEvent::MeetingsChanged);
    let segments = inner.db.segments(meeting_id, false, false)?;
    let speakers = inner.db.speakers(meeting_id)?;
    let names = crate::prompts::speaker_names(&speakers);
    let transcript = crate::prompts::format_transcript(&segments, &names);
    let notes = inner.db.notes(meeting_id)?;
    let requests = inner.db.requests(meeting_id)?;
    let date = meeting_date(&meeting);
    if transcript.trim().is_empty() && notes.is_empty() {
        let md = format!(
            "# {} — {}\n\n## En bref\nAucune parole transcrite pendant cette réunion.\n",
            meeting.title.clone().unwrap_or_else(|| "Réunion".into()),
            date
        );
        let v = inner.db.save_report(meeting_id, &md, None, None, false)?;
        inner.db.set_meeting_status(meeting_id, "done")?;
        inner.emit(CoreEvent::Report { meeting_id: meeting_id.into(), version: v });
        inner.emit(CoreEvent::MeetingsChanged);
        return Ok(());
    }
    let prompt = crate::prompts::report(&meeting, &date, &transcript, &notes, &requests);
    let res = inner
        .run_agent(Some(&meeting), Tier::Summary, ToolAccess::None, SessionMode::None, prompt, 0, None)
        .await;
    let res = match res {
        Ok(r) => r,
        Err(e) => {
            inner.db.set_meeting_status(meeting_id, "error")?;
            inner.emit(CoreEvent::MeetingsChanged);
            return Err(e);
        }
    };
    let (md, json) = split_json(&res.answer);
    let mut parsed: ReportJson = json.and_then(|j| serde_json::from_value(j).ok()).unwrap_or_default();
    if parsed.decisions.is_empty() && parsed.actions.is_empty() {
        let fb = parse_markdown(&md);
        parsed.decisions = fb.decisions;
        parsed.actions = fb.actions;
    }
    let md = ensure_title(&md, &meeting, &date);
    let provider = inner.settings().agent.provider.clone();
    let version = inner.db.save_report(meeting_id, &md, Some(&provider), res.model.as_deref(), false)?;
    inner.db.replace_decisions_actions(
        meeting_id,
        &parsed.decisions,
        &parsed
            .actions
            .iter()
            .filter(|a| !a.text.trim().is_empty())
            .map(|a| (a.owner.clone(), a.text.clone(), a.due.clone()))
            .collect::<Vec<_>>(),
    )?;
    for (label, name) in &parsed.speakers {
        let label_used = speakers.iter().any(|s| &s.label == label);
        if label.starts_with("Eux") && label_used && !name.trim().is_empty() {
            // Proposition : seulement si l'utilisateur n'a rien saisi.
            let existing = speakers.iter().find(|s| &s.label == label).and_then(|s| s.display_name.clone());
            if existing.is_none() {
                inner.db.upsert_speaker(meeting_id, label, Some(&format!("{name} ?")))?;
            }
        }
    }
    inner.db.clear_drafts(meeting_id)?;
    for d in &parsed.drafts {
        if !d.body.trim().is_empty() {
            inner.db.add_draft(meeting_id, &d.kind, d.target.as_deref(), &d.body)?;
        }
    }
    inner.db.set_meeting_status(meeting_id, "done")?;
    inner.emit(CoreEvent::Report { meeting_id: meeting_id.into(), version });
    inner.emit(CoreEvent::MeetingsChanged);
    inner.emit(CoreEvent::Notify {
        title: "📝 Compte rendu prêt".into(),
        body: meeting.title.clone().unwrap_or_else(|| "Réunion".into()),
        route: Some(format!("/meetings/{meeting_id}?tab=report")),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Daily — 26/11/2026\n## En bref\nLigne.\n## Décisions\n- **On garde le verrou de créneau** jusqu'à la v2\n## Actions — qui / quoi / échéance\n- **Marie** — corriger APP-1234 — vendredi\n- **Moi** — relire la MR — —\n## Questions ouvertes\n- Pourquoi 500 ?\n## Points techniques évoqués\n- ms-booking\n\n```json\n{\"decisions\":[\"On garde le verrou de créneau\"],\"actions\":[{\"owner\":\"Marie\",\"text\":\"Corriger APP-1234\",\"due\":\"vendredi\"}],\"speakers\":{\"Eux-1\":\"Marie\"}}\n```\n";

    #[test]
    fn bloc_json() {
        let (md, j) = split_json(SAMPLE);
        assert!(!md.contains("```json"));
        let r: ReportJson = serde_json::from_value(j.unwrap()).unwrap();
        assert_eq!(r.actions[0].owner.as_deref(), Some("Marie"));
        assert_eq!(r.speakers["Eux-1"], "Marie");
    }

    #[test]
    fn repli_markdown() {
        let r = parse_markdown(SAMPLE);
        assert_eq!(r.decisions, vec!["On garde le verrou de créneau jusqu'à la v2"]);
        assert_eq!(r.actions.len(), 2);
        assert_eq!(r.actions[0].owner.as_deref(), Some("Marie"));
        assert_eq!(r.actions[0].due.as_deref(), Some("vendredi"));
        assert_eq!(r.actions[1].due, None);
        assert_eq!(section(SAMPLE, "Points techniques").unwrap(), "- ms-booking\n\n```json\n{\"decisions\":[\"On garde le verrou de créneau\"],\"actions\":[{\"owner\":\"Marie\",\"text\":\"Corriger APP-1234\",\"due\":\"vendredi\"}],\"speakers\":{\"Eux-1\":\"Marie\"}}\n```");
        let without = remove_section(SAMPLE, "Questions ouvertes");
        assert!(!without.contains("Pourquoi 500"));
        assert!(without.contains("## Points techniques"));
    }

    #[test]
    fn resume() {
        assert_eq!(extract_summary("TL;DR: Le verrou est dans `SlotLock.kt:42`.\n\n## Jira\n- APP-1"), "Le verrou est dans `SlotLock.kt:42`.");
        assert_eq!(extract_summary("**TL;DR :** ok\n"), "ok");
        assert_eq!(extract_summary("# Titre\nPremière ligne\nDeuxième\nTroisième"), "Première ligne Deuxième");
    }
}
