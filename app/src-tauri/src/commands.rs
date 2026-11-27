//! Commandes Tauri : l'API de docs/contracts.md §5, branchée sur `scribe-core`.

use crate::AppState;
use scribe_core::doctor::DoctorItem;
use scribe_core::events::LiveState;
use scribe_core::model::*;
use scribe_core::settings::Settings;
use scribe_core::wake::check::WakeCheck;
use scribe_core::wake::variants::VariantCheck;
use serde::Serialize;
use tauri::{AppHandle, State};

type R<T> = Result<T, String>;

fn e(err: impl std::fmt::Display) -> String {
    let s = format!("{err:#}");
    tracing::warn!("commande en échec : {s}");
    s
}

fn er(err: anyhow::Error) -> String {
    e(format!("{err:#}"))
}

// ------------------------------------------------------------------ direct

#[tauri::command]
pub fn live_state(st: State<'_, AppState>) -> LiveState {
    st.core.live_state()
}

#[tauri::command]
pub async fn start_meeting(st: State<'_, AppState>, title: Option<String>) -> R<Meeting> {
    st.core.start_meeting(title).await.map_err(er)
}

#[tauri::command]
pub async fn stop_meeting(st: State<'_, AppState>) -> R<()> {
    st.core.stop_meeting().await.map_err(er)
}

#[tauri::command]
pub fn set_aside(st: State<'_, AppState>, on: bool) {
    st.core.set_aside(on);
}

#[tauri::command]
pub fn set_live_confidential(st: State<'_, AppState>, on: bool) -> R<()> {
    st.core.set_live_confidential(on).map_err(er)
}

#[tauri::command]
pub fn ask(st: State<'_, AppState>, query: String, meeting_id: Option<String>) -> R<Request> {
    st.core.ask(&query, meeting_id.as_deref(), "text").map_err(er)
}

#[tauri::command]
pub fn ask_history(st: State<'_, AppState>, query: String) -> R<Request> {
    st.core.ask_history(&query, "text").map_err(er)
}

#[tauri::command]
pub fn cancel_request(st: State<'_, AppState>, id: String) -> R<()> {
    st.core.cancel_request(&id).map_err(er)
}

#[tauri::command]
pub fn recent_requests(st: State<'_, AppState>, limit: i64) -> R<Vec<Request>> {
    st.core.recent_requests(limit.clamp(1, 200)).map_err(er)
}

// ------------------------------------------------------------------ réunions

#[tauri::command]
pub fn list_meetings(st: State<'_, AppState>, filter: Option<MeetingFilter>) -> R<Vec<MeetingSummary>> {
    st.core.db().list_meetings(&filter.unwrap_or_default()).map_err(er)
}

#[tauri::command]
pub fn get_meeting(st: State<'_, AppState>, id: String) -> R<MeetingDetail> {
    st.core.db().meeting_detail(&id).map_err(er)?.ok_or_else(|| "réunion introuvable".into())
}

#[tauri::command]
pub fn get_transcript(st: State<'_, AppState>, id: String, include_private: bool) -> R<Vec<Segment>> {
    st.core.db().segments(&id, include_private, true).map_err(er)
}

#[tauri::command]
pub fn get_report_version(st: State<'_, AppState>, id: String, version: i64) -> R<Report> {
    st.core.db().get_report_version(&id, version).map_err(er)?.ok_or_else(|| "version introuvable".into())
}

#[tauri::command]
pub fn rename_meeting(st: State<'_, AppState>, id: String, title: String) -> R<()> {
    st.core.rename_meeting(&id, &title).map_err(er)
}

#[tauri::command]
pub fn set_confidential(st: State<'_, AppState>, id: String, on: bool) -> R<()> {
    st.core.set_confidential(&id, on).map_err(er)
}

#[tauri::command]
pub fn delete_meeting(st: State<'_, AppState>, id: String) -> R<()> {
    st.core.delete_meeting(&id).map_err(er)
}

#[tauri::command]
pub fn rename_speaker(st: State<'_, AppState>, meeting_id: String, label: String, display_name: String) -> R<()> {
    st.core.db().rename_speaker(&meeting_id, &label, &display_name).map_err(er)
}

#[tauri::command]
pub fn add_note(st: State<'_, AppState>, meeting_id: String, text: String) -> R<Note> {
    st.core.add_note(&meeting_id, &text).map_err(er)
}

#[tauri::command]
pub fn update_note(st: State<'_, AppState>, id: i64, text: String) -> R<()> {
    st.core.db().update_note(id, &text).map_err(er)
}

#[tauri::command]
pub fn delete_note(st: State<'_, AppState>, id: i64) -> R<()> {
    st.core.db().delete_note(id).map_err(er)
}

#[tauri::command]
pub fn regenerate_report(st: State<'_, AppState>, id: String) -> R<()> {
    st.core.regenerate_report(&id).map_err(er)
}

#[tauri::command]
pub fn correlate(st: State<'_, AppState>, id: String) -> R<()> {
    st.core.correlate(&id).map_err(er)
}

#[tauri::command]
pub fn export_meeting(st: State<'_, AppState>, id: String) -> R<String> {
    st.core.export_meeting(&id).map_err(er)
}

#[tauri::command]
pub fn search(st: State<'_, AppState>, query: String) -> R<Vec<SearchHit>> {
    st.core.db().search(&query, 100).map_err(er)
}

#[tauri::command]
pub fn list_actions(st: State<'_, AppState>, filter: Option<ActionFilter>) -> R<Vec<Action>> {
    st.core.db().list_actions(&filter.unwrap_or_default()).map_err(er)
}

#[tauri::command]
pub fn set_action_status(st: State<'_, AppState>, id: i64, status: String) -> R<()> {
    if status != "open" && status != "done" {
        return Err("statut inconnu".into());
    }
    st.core.db().set_action_status(id, &status).map_err(er)
}

#[tauri::command]
pub fn list_decisions(st: State<'_, AppState>, since: Option<String>) -> R<Vec<Decision>> {
    st.core.db().list_decisions(since.as_deref()).map_err(er)
}

// ------------------------------------------------------------------ réglages

#[tauri::command]
pub fn get_settings(st: State<'_, AppState>) -> Settings {
    st.core.settings()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, st: State<'_, AppState>, settings: Settings) -> R<Settings> {
    let s = st.core.save_settings(settings).map_err(er)?;
    crate::shortcuts::register(&app, &st.core);
    Ok(s)
}

#[tauri::command]
pub fn export_settings(st: State<'_, AppState>) -> R<String> {
    st.core.export_settings().map_err(er)
}

#[tauri::command]
pub fn import_settings(app: AppHandle, st: State<'_, AppState>, toml: String) -> R<Settings> {
    let s = st.core.import_settings(&toml).map_err(er)?;
    crate::shortcuts::register(&app, &st.core);
    Ok(s)
}

#[tauri::command]
pub async fn detect_agents(st: State<'_, AppState>) -> R<Vec<scribe_core::agents::AgentInfo>> {
    Ok(st.core.detect_agents().await)
}

#[derive(Serialize)]
pub struct TestAgentResult {
    ok: bool,
    message: String,
    duration_ms: u128,
}

#[tauri::command]
pub fn list_audio_devices(st: State<'_, AppState>) -> scribe_core::sidecar::AudioDevices {
    st.core.audio_devices()
}

#[tauri::command]
pub async fn list_models(st: State<'_, AppState>, provider: String) -> R<Vec<scribe_core::agents::ModelInfo>> {
    st.core.list_models(&provider).await.map_err(er)
}

#[tauri::command]
pub async fn test_agent(
    st: State<'_, AppState>,
    provider: String,
    model: Option<String>,
    effort: Option<String>,
) -> R<TestAgentResult> {
    let (ok, message, duration_ms) = st.core.test_agent_with(&provider, model, effort).await.map_err(er)?;
    Ok(TestAgentResult { ok, message, duration_ms })
}

#[tauri::command]
pub fn list_connectors(st: State<'_, AppState>) -> R<Vec<Connector>> {
    st.core.list_connectors().map_err(er)
}

#[tauri::command]
pub async fn refresh_connectors(st: State<'_, AppState>) -> R<Vec<Connector>> {
    st.core.refresh_connectors().await.map_err(er)
}

#[tauri::command]
pub fn update_connector(st: State<'_, AppState>, id: i64, use_live: bool, use_correlation: bool) -> R<Connector> {
    st.core.db().update_connector(id, use_live, use_correlation).map_err(er)
}

#[tauri::command]
pub fn list_repos(st: State<'_, AppState>) -> R<Vec<Repo>> {
    st.core.db().list_repos().map_err(er)
}

#[tauri::command]
pub fn add_repo(st: State<'_, AppState>, path: String, label: Option<String>) -> R<Repo> {
    let p = std::path::Path::new(&path);
    if !p.is_dir() {
        return Err(format!("dossier introuvable : {path}"));
    }
    let abs = p.canonicalize().map_err(e)?;
    let label = label
        .filter(|l| !l.trim().is_empty())
        .or_else(|| abs.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_default();
    st.core.db().add_repo(&abs.display().to_string(), &label).map_err(er)
}

#[tauri::command]
pub fn update_repo(st: State<'_, AppState>, repo: Repo) -> R<Repo> {
    st.core.db().update_repo(&repo).map_err(er)
}

#[tauri::command]
pub fn remove_repo(st: State<'_, AppState>, id: i64) -> R<()> {
    st.core.db().remove_repo(id).map_err(er)
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> R<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Choisir un dépôt").pick_folder(move |f| {
        let _ = tx.send(f);
    });
    let f = rx.await.map_err(e)?;
    Ok(f.and_then(|p| p.into_path().ok()).map(|p| p.display().to_string()))
}

// ------------------------------------------------------------------ voix

#[tauri::command]
pub fn check_wake_word(st: State<'_, AppState>, word: String, strict: bool) -> R<WakeCheck> {
    st.core.check_wake_word(&word, strict).map_err(er)
}

#[tauri::command]
pub fn generate_variants(st: State<'_, AppState>, word: String) -> VariantCheck {
    st.core.generate_variants(&word)
}

#[tauri::command]
pub fn check_variants(st: State<'_, AppState>, word: String, variants: Vec<String>) -> VariantCheck {
    st.core.check_variants(&word, &variants)
}

#[derive(Serialize)]
pub struct WakeDetect {
    detected: bool,
    intent: Option<String>,
    query: Option<String>,
}

#[tauri::command]
pub fn wake_detect(st: State<'_, AppState>, text: String) -> WakeDetect {
    match st.core.wake_detect(&text) {
        Some(d) => WakeDetect { detected: true, intent: Some(d.intent.as_str().into()), query: Some(d.query) },
        None => WakeDetect { detected: false, intent: None, query: None },
    }
}

#[derive(Serialize)]
pub struct WakeText {
    text: String,
}

#[tauri::command]
pub async fn wake_test_record(st: State<'_, AppState>, seconds: u32) -> R<WakeText> {
    let seconds = seconds.clamp(1, 60);
    let fut = st.core.wake_test_record(seconds);
    let text = tokio::time::timeout(std::time::Duration::from_secs(seconds as u64 + 30), fut)
        .await
        .map_err(|_| "le micro n'a rien renvoyé à temps".to_string())?
        .map_err(er)?;
    Ok(WakeText { text })
}

// ------------------------------------------------------------------ divers

#[tauri::command]
pub async fn doctor(st: State<'_, AppState>) -> R<Vec<DoctorItem>> {
    Ok(scribe_core::doctor::run(&st.core, false).await)
}

#[tauri::command]
pub fn delete_all_data(st: State<'_, AppState>) -> R<()> {
    st.core.delete_all_data().map_err(er)
}

#[tauri::command]
pub fn log_ui_error(message: String) {
    tracing::error!(target: "ui", "{}", message.chars().take(4000).collect::<String>());
}
