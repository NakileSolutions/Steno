//! Corrélation du compte rendu avec la codebase et les connecteurs (PLAN §7.2).

use crate::agents::{SessionMode, ToolAccess};
use crate::core::Inner;
use crate::events::CoreEvent;
use crate::model::{Meeting, ReportRef, Tier};
use anyhow::{bail, Context, Result};

pub const SECTION: &str = "Liens avec le code et les outils";

/// Pourquoi la corrélation est impossible pour cette réunion (ou `None`).
pub fn blocked_reason(inner: &Inner, meeting: &Meeting) -> Option<String> {
    let s = inner.settings();
    if meeting.confidential {
        return Some("réunion confidentielle : aucun appel IA".into());
    }
    if s.correlation.mode == "off" || meeting.correlation.as_deref() == Some("off") {
        return Some("la corrélation est désactivée".into());
    }
    if s.correlation.is_excluded(meeting.title.as_deref()) {
        return Some("le titre contient un mot d'exclusion".into());
    }
    None
}

/// Faut-il lancer la corrélation automatiquement après le compte rendu ?
pub fn should_auto(inner: &Inner, meeting: &Meeting) -> bool {
    let mode = meeting.correlation.clone().unwrap_or_else(|| inner.settings().correlation.mode.clone());
    mode == "auto" && blocked_reason(inner, meeting).is_none()
}

pub async fn run(inner: &Inner, meeting_id: &str) -> Result<()> {
    let meeting = inner.db.get_meeting(meeting_id)?.context("réunion introuvable")?;
    if let Some(r) = blocked_reason(inner, &meeting) {
        bail!("corrélation impossible : {r}");
    }
    let report = inner.db.get_report(meeting_id)?.context("aucun compte rendu à corréler")?;
    let sources = inner.sources(false)?;
    let connectors = sources.connectors.clone();
    let budget = inner.settings().correlation.budget;
    let mut src = sources.clone();
    src.budget = budget;
    let base_md = crate::report::remove_section(&report.markdown, SECTION);
    let prompt = crate::prompts::correlate(&meeting, &base_md, &src);
    inner.emit(CoreEvent::Notify {
        title: "🔗 Corrélation en cours".into(),
        body: meeting.title.clone().unwrap_or_default(),
        route: None,
    });
    let res = inner
        .run_agent(
            Some(&meeting),
            Tier::Search,
            ToolAccess::ReadOnly { connectors },
            SessionMode::None,
            prompt,
            budget,
            Some(sources.repo_paths()),
        )
        .await?;
    let (md, json) = crate::report::split_json(&res.answer);
    let refs: Vec<ReportRef> = json
        .and_then(|j| j.get("refs").cloned())
        .and_then(|r| serde_json::from_value(r).ok())
        .unwrap_or_default();
    let section_body = crate::report::section(&md, SECTION).unwrap_or_else(|| {
        // L'agent n'a pas mis le titre : tout son texte est la section.
        md.trim().to_string()
    });
    let new_md = format!("{base_md}\n\n## {SECTION}\n\n{section_body}\n");
    let version = inner.db.save_report(
        meeting_id,
        &new_md,
        Some(&inner.settings().agent.agent_for(Tier::Search)),
        res.model.as_deref(),
        true,
    )?;
    inner.db.replace_refs(meeting_id, &refs)?;
    inner.emit(CoreEvent::Report { meeting_id: meeting_id.into(), version });
    inner.emit(CoreEvent::MeetingsChanged);
    inner.emit(CoreEvent::Notify {
        title: "🔗 Corrélation terminée".into(),
        body: format!("{} lien(s) trouvé(s)", refs.len()),
        route: Some(format!("/meetings/{meeting_id}?tab=report")),
    });
    Ok(())
}
