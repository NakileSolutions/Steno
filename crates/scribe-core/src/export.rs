//! Exports d'une réunion : markdown complet (compte rendu, actions, transcription sans
//! apartés) et transcription seule (texte, sous-titres SRT / WebVTT).

use crate::db::Db;
use crate::model::Segment;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Formats d'export : `markdown` (complet), `txt`, `srt`, `vtt` (transcription seule).
pub const FORMATS: [&str; 4] = ["markdown", "txt", "srt", "vtt"];

pub fn extension(format: &str) -> Result<&'static str> {
    Ok(match format {
        "markdown" => "md",
        "txt" => "txt",
        "srt" => "srt",
        "vtt" => "vtt",
        other => bail!("format d'export inconnu : {other}"),
    })
}

/// Contenu de l'export au format demandé.
pub fn meeting_export(db: &Db, id: &str, format: &str) -> Result<String> {
    match format {
        "markdown" => meeting_markdown(db, id),
        "txt" | "srt" | "vtt" => {
            let d = db.meeting_detail(id)?.context("réunion introuvable")?;
            let names = crate::prompts::speaker_names(&d.speakers);
            let segs: Vec<Segment> = db.segments(id, false, false)?;
            Ok(match format {
                "txt" => {
                    let title = d.meeting.title.clone().unwrap_or_else(|| "Réunion".into());
                    format!(
                        "{title} — {}\n\n{}\n",
                        crate::report::meeting_date(&d.meeting),
                        crate::prompts::format_transcript(&segs, &names)
                    )
                }
                "srt" => subtitles(&segs, &names, false),
                _ => subtitles(&segs, &names, true),
            })
        }
        other => bail!("format d'export inconnu : {other}"),
    }
}

/// Nom de fichier proposé : `2026-11-27-point-dev.md`.
pub fn file_name(db: &Db, id: &str, format: &str) -> Result<String> {
    let m = db.get_meeting(id)?.context("réunion introuvable")?;
    let date = m.started_at.get(..10).unwrap_or("reunion");
    let title = crate::text::slugify(m.title.as_deref().unwrap_or("reunion"));
    let suffix = if format == "markdown" { "" } else { "-transcription" };
    Ok(format!("{date}-{}{suffix}.{}", if title.is_empty() { "reunion" } else { &title }, extension(format)?))
}

/// Horodatage de sous-titre : `00:01:02,345` (SRT) ou `00:01:02.345` (WebVTT).
fn stamp(t: f64, vtt: bool) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    let (h, m, s, ms) = (ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000);
    format!("{h:02}:{m:02}:{s:02}{}{ms:03}", if vtt { '.' } else { ',' })
}

/// Sous-titres : une entrée par phrase (apartés et échos exclus), nom de l'intervenant en tête.
fn subtitles(segments: &[Segment], names: &HashMap<String, String>, vtt: bool) -> String {
    let mut out = String::new();
    if vtt {
        out.push_str("WEBVTT\n\n");
    }
    let mut n = 0;
    let mut last_end = 0.0_f64;
    for s in segments.iter().filter(|s| !s.private && !s.is_echo) {
        let t0 = s.t0.unwrap_or(last_end);
        let t1 = s.t1.unwrap_or(t0 + 2.0).max(t0 + 0.5);
        last_end = t1;
        let who = names.get(&s.speaker).cloned().unwrap_or_else(|| display_label(&s.speaker));
        n += 1;
        if vtt {
            out.push_str(&format!("{} --> {}\n<v {who}>{}\n\n", stamp(t0, true), stamp(t1, true), s.text.trim()));
        } else {
            out.push_str(&format!("{n}\n{} --> {}\n{who} : {}\n\n", stamp(t0, false), stamp(t1, false), s.text.trim()));
        }
    }
    out
}

/// « Eux-3 » → « Intervenant 3 » (comme dans l'interface).
pub fn display_label(label: &str) -> String {
    match label.strip_prefix("Eux-") {
        Some(n) if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => format!("Intervenant {n}"),
        _ => label.to_string(),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(t0: f64, t1: f64, speaker: &str, text: &str) -> Segment {
        Segment {
            id: 0,
            meeting_id: "m".into(),
            seq: 0,
            ts: String::new(),
            t0: Some(t0),
            t1: Some(t1),
            channel: "them".into(),
            speaker: speaker.into(),
            text: text.into(),
            private: false,
            is_echo: false,
            request_id: None,
        }
    }

    #[test]
    fn sous_titres_srt_et_vtt() {
        let mut names = HashMap::new();
        names.insert("Eux-1".to_string(), "Marie".to_string());
        let mut aparte = seg(4.0, 5.0, "Moi", "secret");
        aparte.private = true;
        let segs = vec![seg(1.5, 3.25, "Eux-1", "Bonjour."), aparte, seg(3661.0, 3662.0, "Eux-2", "Oui.")];
        let srt = subtitles(&segs, &names, false);
        assert!(srt.starts_with("1\n00:00:01,500 --> 00:00:03,250\nMarie : Bonjour.\n"));
        assert!(srt.contains("2\n01:01:01,000 --> 01:01:02,000\nIntervenant 2 : Oui."));
        assert!(!srt.contains("secret"));
        let vtt = subtitles(&segs, &names, true);
        assert!(vtt.starts_with("WEBVTT\n\n00:00:01.500 --> 00:00:03.250\n<v Marie>Bonjour."));
    }

    #[test]
    fn formats_connus() {
        for f in FORMATS {
            assert!(extension(f).is_ok());
        }
        assert!(extension("pdf").is_err());
    }
}
