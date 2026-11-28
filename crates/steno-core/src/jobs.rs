//! File de tâches persistante (PLAN §5.2) : `request`, `report`, `correlate`, `diarize`,
//! `export`, `control` (stop / cancel typed in another process).
//! Les tâches IA sont limitées à `agent.max_parallel` en parallèle.

use crate::agents::{SessionMode, ToolAccess};
use crate::core::Inner;
use crate::events::CoreEvent;
use crate::model::{Intent, Job, Tier};
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const AI_KINDS: &[&str] = &["request", "report", "correlate"];
/// A `diarize` job retranscribes the meeting with the final model, then diarizes it: both
/// load large models in the audio sidecar and read the whole recording. Two meetings
/// ending close together must not stack them in memory, so these jobs run one at a time.
const DIARIZE_KINDS: &[&str] = &["diarize"];
const MAX_PARALLEL_DIARIZE: usize = 1;
const OTHER_KINDS: &[&str] = &["control", "export"];

/// Lance la boucle de traitement en tâche de fond.
pub fn spawn_runner(inner: Arc<Inner>) {
    tokio::spawn(async move {
        let running_ai = Arc::new(AtomicUsize::new(0));
        let running_diarize = Arc::new(AtomicUsize::new(0));
        loop {
            let max = inner.settings().agent.max_parallel.max(1) as usize;
            // Tâches IA dans la limite de parallélisme.
            spawn_bounded(&inner, AI_KINDS, &running_ai, max);
            spawn_bounded(&inner, DIARIZE_KINDS, &running_diarize, MAX_PARALLEL_DIARIZE);
            while let Ok(Some(job)) = inner.db.claim_job(OTHER_KINDS) {
                let i = inner.clone();
                tokio::spawn(async move {
                    execute(&i, job).await;
                    i.jobs_notify.notify_one();
                });
            }
            tokio::select! {
                _ = inner.jobs_notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                _ = inner.shutdown.cancelled() => break,
            }
        }
    });
}

/// Claims pending jobs of `kinds` while fewer than `max` of them are running, and runs each
/// one in its own task; `running` counts them until they finish.
fn spawn_bounded(inner: &Arc<Inner>, kinds: &[&str], running: &Arc<AtomicUsize>, max: usize) {
    while running.load(Ordering::SeqCst) < max {
        match inner.db.claim_job(kinds) {
            Ok(Some(job)) => {
                running.fetch_add(1, Ordering::SeqCst);
                let (i, r) = (inner.clone(), running.clone());
                tokio::spawn(async move {
                    execute(&i, job).await;
                    r.fetch_sub(1, Ordering::SeqCst);
                    i.jobs_notify.notify_one();
                });
            }
            Ok(None) => break,
            Err(e) => {
                tracing::error!("file de tâches : {e:#}");
                break;
            }
        }
    }
}

/// Exécute toutes les tâches en attente puis rend la main (CLI sans app).
pub async fn drain(inner: Arc<Inner>) -> Result<usize> {
    let mut n = 0;
    loop {
        let mut kinds = AI_KINDS.to_vec();
        kinds.extend_from_slice(DIARIZE_KINDS);
        kinds.extend_from_slice(OTHER_KINDS);
        let Some(job) = inner.db.claim_job(&kinds)? else { break };
        execute(&inner, job).await;
        n += 1;
    }
    Ok(n)
}

pub(crate) async fn execute(inner: &Inner, job: Job) {
    let id = job.id;
    let kind = job.kind.clone();
    tracing::info!("tâche {id} ({kind}) démarrée");
    let res = match kind.as_str() {
        "request" => run_request(inner, &job).await,
        "control" => run_control(inner, &job),
        "report" => run_report(inner, &job).await,
        "correlate" => {
            let mid = job.meeting_id.clone().unwrap_or_default();
            crate::correlate::run(inner, &mid).await
        }
        "diarize" => run_diarize(inner, &job).await,
        "export" => run_export(inner, &job),
        other => Err(anyhow::anyhow!("type de tâche inconnu : {other}")),
    };
    match res {
        Ok(()) => {
            let _ = inner.db.finish_job(id, "done", None);
        }
        Err(e) => {
            let msg = format!("{e:#}");
            tracing::warn!("tâche {id} ({kind}) en échec : {msg}");
            let status = if msg.contains("annulée") { "cancelled" } else { "error" };
            let _ = inner.db.finish_job(id, status, Some(&msg));
            if kind == "correlate" || kind == "report" {
                inner.emit(CoreEvent::Notify {
                    title: if kind == "report" { inner.tr("⚠️ Compte rendu en échec", "⚠️ Summary failed") } else { inner.tr("⚠️ Corrélation en échec", "⚠️ Correlation failed") },
                    body: crate::text::truncate(&crate::i18n_messages::localize(inner.lang(), &msg), 200),
                    route: job.meeting_id.as_ref().map(|m| format!("/meetings/{m}")),
                });
            }
        }
    }
}

/// Contexte d'une demande : les `seconds` dernières secondes de transcription avant elle.
fn context_for(inner: &Inner, meeting_id: &str, created_at: &str, seconds: i64) -> Result<String> {
    let segs = inner.db.segments(meeting_id, false, false)?;
    let created = chrono::DateTime::parse_from_rfc3339(created_at).ok();
    let selected: Vec<_> = segs
        .into_iter()
        .filter(|s| match (created, chrono::DateTime::parse_from_rfc3339(&s.ts).ok()) {
            (Some(c), Some(t)) => {
                let d = (c - t).num_seconds();
                (-5..=seconds).contains(&d)
            }
            _ => true,
        })
        .collect();
    let speakers = inner.db.speakers(meeting_id)?;
    Ok(crate::prompts::format_transcript(&selected, &crate::prompts::speaker_names(&speakers)))
}

async fn run_request(inner: &Inner, job: &Job) -> Result<()> {
    let rid = job.payload.get("request_id").and_then(|v| v.as_str()).context("request_id manquant")?.to_string();
    let Some(req) = inner.db.get_request(&rid)? else { return Ok(()) };
    if req.status != "queued" && req.status != "running" {
        return Ok(());
    }
    let meeting = match &req.meeting_id {
        Some(id) => inner.db.get_meeting(id)?,
        None => None,
    };
    if meeting.as_ref().map(|m| m.confidential).unwrap_or(false) {
        inner.db.finish_request(&rid, "refused", Some("Réunion confidentielle : aucune recherche."), None, &[], None)?;
        emit_request(inner, &rid);
        return Ok(());
    }
    // Register the cancellation token before marking the request running: a cancellation
    // either finds the token or lands first, and then the guarded update refuses to run it.
    let cancel = CancellationToken::new();
    inner.cancels.lock().unwrap().insert(rid.clone(), cancel.clone());
    if !inner.db.set_request_running(&rid)? {
        inner.cancels.lock().unwrap().remove(&rid);
        return Ok(());
    }
    emit_request(inner, &rid);
    let started = std::time::Instant::now();
    let s = inner.settings();
    let intent = Intent::parse(&req.intent).unwrap_or(Intent::Search);
    let adapter_caps = inner.adapter_for_tier(intent.tier())?.capabilities();
    let now = chrono::Local::now().format("%H:%M").to_string();
    let result = match intent {
        Intent::Recap => {
            let m = meeting.as_ref().context("aucune réunion à récapituler")?;
            let segs = inner.db.segments(&m.id, false, false)?;
            let names = crate::prompts::speaker_names(&inner.db.speakers(&m.id)?);
            let prompt = crate::prompts::recap(m, &crate::prompts::format_transcript(&segs, &names), &inner.db.notes(&m.id)?)
                + crate::i18n::answer_language(inner.lang());
            inner
                .run_agent_with(Some(m), Tier::Quick, ToolAccess::None, SessionMode::New(String::new()), prompt, 0, None, cancel.clone())
                .await
        }
        Intent::History => {
            let hits = inner.db.search_relevant(&req.query, 20)?;
            let mut ids: Vec<String> = vec![];
            for h in &hits {
                if !ids.contains(&h.meeting_id) {
                    ids.push(h.meeting_id.clone());
                }
            }
            let mut reports = vec![];
            for id in ids.iter().take(3) {
                if let (Some(r), Some(m)) = (inner.db.get_report(id)?, inner.db.get_meeting(id)?) {
                    reports.push((
                        format!("{} — {}", m.title.unwrap_or_default(), m.started_at.get(..10).unwrap_or("")),
                        r.markdown,
                    ));
                }
            }
            let prompt = crate::prompts::history(&req.query, &hits, &reports) + crate::i18n::answer_language(inner.lang());
            inner
                .run_agent_with(None, Tier::Search, ToolAccess::None, SessionMode::None, prompt, 0, None, cancel.clone())
                .await
        }
        _ => {
            let sources = inner.sources(true)?;
            let context = match &meeting {
                Some(m) => context_for(inner, &m.id, &req.created_at, req.context_seconds.unwrap_or(120))?,
                None => String::new(),
            };
            let notes = match &meeting {
                Some(m) => inner.db.notes(&m.id)?,
                None => vec![],
            };
            let mut prompt = crate::prompts::live_request(meeting.as_ref(), intent, &req.query, &context, &notes, &sources, &now);
            prompt.push_str(crate::i18n::answer_language(inner.lang()));
            // L'agent ne sait pas reprendre une session : on rejoue les échanges précédents.
            if !adapter_caps.resume_session {
                if let Some(m) = &meeting {
                    let prev: Vec<String> = inner
                        .db
                        .requests(&m.id)?
                        .into_iter()
                        .filter(|r| r.id != rid && r.status == "done" && r.summary.is_some())
                        .map(|r| format!("- {} → {}", r.query, r.summary.unwrap_or_default()))
                        .collect();
                    if !prev.is_empty() {
                        prompt.push_str(&format!("\n## Échanges précédents dans cette réunion\n{}\n", prev.join("\n")));
                    }
                }
            }
            let repos = sources.repo_paths();
            let connectors = sources.connectors.clone();
            inner
                .run_agent_with(
                    meeting.as_ref(),
                    Tier::Search,
                    ToolAccess::ReadOnly { connectors },
                    if meeting.is_some() { SessionMode::New(String::new()) } else { SessionMode::None },
                    prompt,
                    s.agent.live_tool_budget,
                    Some(repos),
                    cancel.clone(),
                )
                .await
        }
    };
    inner.cancels.lock().unwrap().remove(&rid);
    // La demande a pu être annulée entre-temps.
    if inner.db.get_request(&rid)?.map(|r| r.status == "cancelled").unwrap_or(false) {
        return Ok(());
    }
    match result {
        Ok(r) => {
            // Mesure : durée totale et outils, pour repérer les recherches lentes dans le journal.
            tracing::info!(
                "demande {rid} ({}) : {:.1} s, {} appel(s) d'outils {:?}",
                req.intent,
                started.elapsed().as_secs_f64(),
                r.tools_used.len(),
                r.tools_used
            );
            let summary = crate::report::extract_summary(&r.answer);
            inner.db.finish_request(&rid, "done", Some(&summary), Some(&r.answer), &r.tools_used, None)?;
            emit_request(inner, &rid);
            let route = match &req.meeting_id {
                Some(m) => format!("/meetings/{m}?tab=requests&request={rid}"),
                None => format!("/search?request={rid}"),
            };
            inner.emit(CoreEvent::Notify { title: "✅ Sténo".into(), body: summary, route: Some(route) });
            Ok(())
        }
        Err(e) => {
            let msg = format!("{e:#}");
            let status = if msg.contains("annulée") { "cancelled" } else { "error" };
            inner.db.finish_request(&rid, status, None, None, &[], Some(&msg))?;
            emit_request(inner, &rid);
            if status == "error" {
                inner.emit(CoreEvent::Notify {
                    title: inner.tr("⚠️ Demande en échec", "⚠️ Request failed"),
                    body: crate::text::truncate(&crate::i18n_messages::localize(inner.lang(), &msg), 200),
                    route: None,
                });
            }
            Err(e)
        }
    }
}

/// « arrête l'écoute » / « annule » typed in another process (`steno ask`): run here, where
/// the live session and the agents' cancellation tokens are.
fn run_control(inner: &Inner, job: &Job) -> Result<()> {
    let rid = job.payload.get("request_id").and_then(|v| v.as_str()).context("request_id manquant")?.to_string();
    let Some(req) = inner.db.get_request(&rid)? else { return Ok(()) };
    let Some(intent) = Intent::parse(&req.intent) else { return Ok(()) };
    if req.status != "queued" || !inner.db.set_request_running(&rid)? {
        return Ok(());
    }
    let (summary, answer) = match crate::core::run_control(inner, intent, req.meeting_id.as_deref()) {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(()),
        Err(e) => {
            inner.db.finish_request(&rid, "error", None, None, &[], Some(&format!("{e:#}")))?;
            emit_request(inner, &rid);
            return Err(e);
        }
    };
    inner.db.finish_request(&rid, "done", Some(&summary), Some(&answer), &[], None)?;
    emit_request(inner, &rid);
    inner.emit(CoreEvent::Notify { title: "Sténo".into(), body: summary, route: None });
    Ok(())
}

fn emit_request(inner: &Inner, id: &str) {
    if let Ok(Some(r)) = inner.db.get_request(id) {
        inner.emit(CoreEvent::Request(r));
    }
}

async fn run_report(inner: &Inner, job: &Job) -> Result<()> {
    let mid = job.meeting_id.clone().context("réunion manquante")?;
    crate::report::generate(inner, &mid).await?;
    let meeting = inner.db.get_meeting(&mid)?.context("réunion introuvable")?;
    // Suppression de l'audio selon la rétention.
    if inner.settings().privacy.audio_retention_days == 0 {
        let _ = std::fs::remove_dir_all(crate::paths::cache_dir().join(&mid));
    }
    if crate::correlate::should_auto(inner, &meeting) && !inner.db.has_job("correlate", &mid, &["pending", "running"])? {
        inner.db.enqueue_job("correlate", Some(&mid), &serde_json::json!({}))?;
    }
    if inner.settings().privacy.auto_export_dir.as_deref().map(|d| !d.trim().is_empty()).unwrap_or(false) {
        inner.db.enqueue_job("export", Some(&mid), &serde_json::json!({}))?;
    }
    inner.jobs_notify.notify_one();
    Ok(())
}

async fn run_diarize(inner: &Inner, job: &Job) -> Result<()> {
    let mid = job.meeting_id.clone().context("réunion manquante")?;
    // Retranscription d'abord (même audio, mêmes bornes) : le compte rendu lira le texte final.
    match crate::live::retranscribe(inner, &mid).await {
        Ok(n) if n > 0 => tracing::info!("réunion {mid} : {n} phrases retranscrites"),
        Ok(_) => {}
        Err(e) => tracing::warn!("retranscription impossible ({e:#}) : on garde la transcription du direct"),
    }
    let res = crate::live::diarize(inner, &mid).await;
    // Réunion supprimée pendant le traitement : plus rien à faire.
    if !inner.db.meeting_exists(&mid)? {
        tracing::info!("réunion {mid} supprimée pendant la diarisation : compte rendu abandonné");
        return Ok(());
    }
    if let Err(e) = &res {
        tracing::warn!("diarisation impossible ({e:#}) : on garde « Eux »");
    }
    // Le compte rendu suit toujours, diarisation réussie ou non.
    if !inner.db.has_job("report", &mid, &["pending", "running"])? {
        // Délai laissé pour ajouter contexte et documents (« Rédiger maintenant » le court-circuite).
        let start = match inner.db.get_meeting(&mid)? {
            Some(m) => crate::report::scheduled_start(inner, &m)?,
            None => None,
        };
        inner.db.enqueue_job_at("report", Some(&mid), &serde_json::json!({}), start.as_deref())?;
        inner.jobs_notify.notify_one();
    }
    Ok(())
}

fn run_export(inner: &Inner, job: &Job) -> Result<()> {
    let mid = job.meeting_id.clone().context("réunion manquante")?;
    let Some(dir) = inner.settings().privacy.auto_export_dir.filter(|d| !d.trim().is_empty()) else {
        return Ok(());
    };
    crate::export::export_to_dir(&inner.db, &mid, std::path::Path::new(&dir))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Core;
    use crate::db::Db;

    fn status(core: &Core, id: i64) -> String {
        core.db().get_job(id).unwrap().unwrap().status
    }

    /// Two meetings ending close together: the runner starts one diarization and leaves the
    /// other pending until it finishes, so the sidecar never holds both sets of models.
    #[tokio::test]
    async fn diarizations_run_one_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let core = Core::new(Db::open(&dir.path().join("steno.db")).unwrap(), Default::default()).unwrap();
        let first = core.db().enqueue_job("diarize", Some("m1"), &serde_json::json!({})).unwrap();
        let second = core.db().enqueue_job("diarize", Some("m2"), &serde_json::json!({})).unwrap();
        // Nothing awaits below, so the spawned job is claimed but never polled.
        let running = Arc::new(AtomicUsize::new(0));
        spawn_bounded(&core.inner, DIARIZE_KINDS, &running, MAX_PARALLEL_DIARIZE);
        assert_eq!(running.load(Ordering::SeqCst), 1);
        assert_eq!((status(&core, first).as_str(), status(&core, second).as_str()), ("running", "pending"));
        // Next pass of the runner while the first one still runs: the second keeps waiting.
        spawn_bounded(&core.inner, DIARIZE_KINDS, &running, MAX_PARALLEL_DIARIZE);
        assert_eq!(status(&core, second), "pending");
        // Control and export jobs never wait behind a diarization.
        assert!(!OTHER_KINDS.contains(&"diarize"));
    }
}
