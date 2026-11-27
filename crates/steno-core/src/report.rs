//! Compte rendu de fin de réunion : génération et analyse de la sortie de l'agent (PLAN §7.1).

use crate::agents::{SessionMode, ToolAccess};
use crate::core::Inner;
use crate::events::CoreEvent;
use crate::model::{Meeting, Tier};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

/// Modèle de compte rendu : consignes ajoutées à la procédure de compte rendu.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub builtin: bool,
}

pub struct BuiltinTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub instructions: &'static str,
}

/// Modèles intégrés. `standard` n'ajoute rien à la procédure.
pub const BUILTIN_TEMPLATES: &[BuiltinTemplate] = &[
    BuiltinTemplate { id: "standard", name: "Standard", description: "Décisions, actions, questions ouvertes, points techniques.", instructions: "" },
    BuiltinTemplate {
        id: "daily",
        name: "Daily / point d'équipe",
        description: "Court : par personne, ce qui est fait, en cours, bloqué.",
        instructions: "Réunion courte et récurrente. « En bref » tient en une ligne. Ajoute après « En bref » une section \
            « ## Tour de table » : une puce par personne, `**Nom** — fait : … ; en cours : … ; blocages : …`. \
            Mets les blocages en évidence. Reste bref dans les autres sections et omets « Points techniques évoqués » s'il n'y a rien de notable.",
    },
    BuiltinTemplate {
        id: "client",
        name: "Point client",
        description: "Présentable au client : demandes, engagements, prochaines étapes.",
        instructions: "Compte rendu destiné à être partagé avec le client : ton neutre et professionnel, aucune remarque interne \
            ni familiarité. Ajoute après « Décisions » une section « ## Demandes du client » et une section « ## Engagements pris » \
            (qui s'engage à quoi, pour quand), puis « ## Prochaines étapes ». Distingue ce que le client a validé de ce qui reste à valider.",
    },
    BuiltinTemplate {
        id: "atelier",
        name: "Atelier / spécification",
        description: "Détaillé : besoins, règles de gestion, cas limites, points à trancher.",
        instructions: "Atelier de conception : sois détaillé et précis. Ajoute après « Décisions » les sections « ## Besoins exprimés », \
            « ## Règles de gestion » (une règle par puce, formulée de façon testable, avec les valeurs chiffrées citées) et \
            « ## Cas limites et exceptions ». Les « Questions ouvertes » indiquent qui doit trancher.",
    },
    BuiltinTemplate {
        id: "arbitrage",
        name: "Comité / arbitrage",
        description: "Options étudiées, arguments, décision et qui l'a prise.",
        instructions: "Réunion de décision. Ajoute avant « Décisions » une section « ## Options étudiées » : pour chaque option, \
            ses arguments pour et contre tels qu'exprimés. Chaque décision précise qui l'a prise ou validée et sa justification.",
    },
    BuiltinTemplate {
        id: "mail",
        name: "Mail de synthèse",
        description: "Prêt à envoyer : objet, synthèse, décisions, actions.",
        instructions: "Rédige le compte rendu sous forme de mail prêt à envoyer aux participants, à la place du format habituel : \
            un titre `# Objet : …`, une formule d'appel courte, un paragraphe de synthèse, puis « ## Décisions », \
            « ## Actions — qui / quoi / échéance » et « ## Prochaines étapes », et une formule de politesse. Ton cordial et professionnel, \
            aucune remarque interne ; omets les sections techniques et les recherches.",
    },
    BuiltinTemplate {
        id: "slack",
        name: "Message Slack / Teams",
        description: "Court, à coller dans un canal : 10 lignes au plus.",
        instructions: "Rédige un message court à coller dans Slack ou Teams, à la place du format habituel : un titre `# …`, \
            puis 10 lignes au plus : 2 lignes de synthèse, les décisions et les actions (`**Qui** — quoi — échéance`) en puces. \
            Pas d'autre section.",
    },
];

/// Tous les modèles : intégrés puis perso.
pub fn templates(s: &crate::settings::Settings) -> Vec<Template> {
    let mut out: Vec<Template> = BUILTIN_TEMPLATES
        .iter()
        .map(|t| Template {
            id: t.id.into(),
            name: t.name.into(),
            description: t.description.into(),
            instructions: t.instructions.into(),
            builtin: true,
        })
        .collect();
    for t in &s.report.templates {
        out.push(Template {
            id: t.id.clone(),
            name: t.name.clone(),
            description: crate::text::truncate(&t.instructions, 90),
            instructions: t.instructions.clone(),
            builtin: false,
        });
    }
    out
}

/// Modèle par identifiant.
pub fn template(s: &crate::settings::Settings, id: &str) -> Option<Template> {
    templates(s).into_iter().find(|t| t.id == id)
}


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
            let h = crate::text::normalize(t.trim_start_matches('#').trim());
            let matches = |p: &str| h.starts_with(&crate::text::normalize(p));
            if matches(heading_prefix) || crate::i18n::heading_aliases(heading_prefix).iter().any(|a| matches(a)) {
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
            let h = crate::text::normalize(t.trim_start_matches('#').trim());
            let matches = |p: &str| h.starts_with(&crate::text::normalize(p));
            skipping = t.starts_with("## ")
                && (matches(heading_prefix) || crate::i18n::heading_aliases(heading_prefix).iter().any(|a| matches(a)));
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
            let body = lc.split_once(':').map(|x| x.1).unwrap_or("").replace("**", "").trim().to_string();
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
/// Marge laissée après une modification du contexte avant que la rédaction ne parte.
pub const CONTEXT_EDIT_MARGIN_S: i64 = 30;

fn to_utc(iso: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(iso).ok().map(|d| d.with_timezone(&chrono::Utc))
}

fn utc_str(d: chrono::DateTime<chrono::Utc>) -> String {
    d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Heure de départ de la rédaction après la fin de la réunion : fin + délai de contexte, et au
/// moins 30 s après la dernière modification du contexte. `None` = tout de suite.
pub fn scheduled_start(inner: &Inner, m: &Meeting) -> Result<Option<String>> {
    if inner.report_now.lock().unwrap().remove(&m.id) {
        return Ok(None);
    }
    let delay = inner.settings().report.context_delay_s as i64;
    if delay == 0 {
        return Ok(None);
    }
    let now = chrono::Utc::now();
    let mut start = m.ended_at.as_deref().and_then(to_utc).map(|e| e + chrono::Duration::seconds(delay));
    if let Some(t) = inner.db.context_updated_at(&m.id)?.as_deref().and_then(to_utc) {
        let t = t + chrono::Duration::seconds(CONTEXT_EDIT_MARGIN_S);
        start = Some(start.map_or(t, |s| s.max(t)));
    }
    Ok(start.filter(|s| *s > now).map(utc_str))
}

/// Une modification du contexte repousse une rédaction encore en attente (au moins 30 s).
pub fn context_touched(inner: &Inner, meeting_id: &str) -> Result<()> {
    inner.db.touch_meeting_context(meeting_id)?;
    if let Some(Some(at)) = inner.db.pending_job_start("report", meeting_id)? {
        let min = crate::db::queries_utc_in(CONTEXT_EDIT_MARGIN_S);
        if at < min {
            inner.db.set_job_start("report", meeting_id, Some(&min))?;
        }
    }
    Ok(())
}

/// Heure de départ prévue de la rédaction si elle attend encore (pour le compte à rebours).
pub fn report_at(inner: &Inner, m: &Meeting) -> Result<Option<String>> {
    if let Some(start) = inner.db.pending_job_start("report", &m.id)? {
        return Ok(start.filter(|s| *s > crate::db::queries_utc_in(0)));
    }
    // Retranscription et diarisation en cours : la rédaction suivra au plus tôt à cette heure.
    if inner.db.has_job("diarize", &m.id, &["pending", "running"])? && !inner.report_now.lock().unwrap().contains(&m.id) {
        let delay = inner.settings().report.context_delay_s as i64;
        if delay > 0 {
            let mut start = m.ended_at.as_deref().and_then(to_utc).map(|e| e + chrono::Duration::seconds(delay));
            if let Some(t) = inner.db.context_updated_at(&m.id)?.as_deref().and_then(to_utc) {
                let t = t + chrono::Duration::seconds(CONTEXT_EDIT_MARGIN_S);
                start = Some(start.map_or(t, |s| s.max(t)));
            }
            return Ok(start.filter(|s| *s > chrono::Utc::now()).map(utc_str));
        }
    }
    Ok(None)
}

/// Contexte, documents et modèle de la réunion, pour la demande de compte rendu.
pub fn report_context(inner: &Inner, meeting: &Meeting) -> Result<crate::prompts::ReportContext> {
    let c = inner.db.meeting_context(&meeting.id)?;
    let s = inner.settings();
    let id = c.template.clone().unwrap_or_else(|| s.report.default_template.clone());
    let template = template(&s, &id).or_else(|| template(&s, "standard")).map(|t| (t.name, t.instructions));
    Ok(crate::prompts::ReportContext {
        text: c.text,
        docs: inner.db.context_texts(&meeting.id)?,
        template,
        lang: s.lang().to_string(),
        participants: inner.db.participants(&meeting.id)?.into_iter().map(|p| p.name).collect(),
    })
}

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
    let ctx = report_context(inner, &meeting)?;
    let prompt = crate::prompts::report(&meeting, &date, &transcript, &notes, &requests, &ctx);
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
    let provider = inner.settings().agent.agent_for(Tier::Summary);
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
        title: inner.tr("📝 Compte rendu prêt", "📝 Summary ready"),
        body: meeting.title.clone().unwrap_or_else(|| inner.tr("Réunion", "Meeting")),
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
