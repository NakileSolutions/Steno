//! Export markdown d'une réunion (compte rendu, actions, transcription sans apartés).

use crate::db::Db;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn meeting_markdown(db: &Db, id: &str) -> Result<String> {
    let d = db.meeting_detail(id)?.context("réunion introuvable")?;
    let m = &d.meeting;
    let date = crate::report::meeting_date(m);
    let title = m.title.clone().unwrap_or_else(|| "Réunion".into());
    let mut out = String::new();
    match &d.report {
        Some(r) => {
            out.push_str(r.markdown.trim());
            out.push('\n');
        }
        None => out.push_str(&format!("# {title} — {date}\n\n_(pas encore de compte rendu)_\n")),
    }
    if !d.actions.is_empty() && !out.contains("## Actions") {
        out.push_str("\n## Actions\n");
        for a in &d.actions {
            let check = if a.status == "done" { "x" } else { " " };
            out.push_str(&format!(
                "- [{check}] **{}** — {} — {}\n",
                a.owner.clone().unwrap_or_else(|| "?".into()),
                a.text,
                a.due.clone().unwrap_or_else(|| "—".into())
            ));
        }
    }
    if !d.refs.is_empty() {
        out.push_str("\n## Références\n");
        for r in &d.refs {
            let v = if r.verified { "vérifié" } else { "supposé" };
            let link = r.url.clone().map(|u| format!(" <{u}>")).unwrap_or_default();
            out.push_str(&format!("- [{}] `{}` {}{} ({v})\n", r.kind, r.reference, r.title.clone().unwrap_or_default(), link));
        }
    }
    let segs = db.segments(id, false, false)?;
    if !segs.is_empty() {
        let names = crate::prompts::speaker_names(&d.speakers);
        out.push_str("\n---\n\n## Transcription\n\n");
        for line in crate::prompts::format_transcript(&segs, &names).lines() {
            out.push_str(line);
            out.push_str("  \n");
        }
    }
    Ok(out)
}

/// Écrit `<dossier>/<id>.md` et renvoie le chemin.
pub fn export_to_dir(db: &Db, id: &str, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let p = dir.join(format!("{id}.md"));
    std::fs::write(&p, meeting_markdown(db, id)?)?;
    Ok(p)
}
