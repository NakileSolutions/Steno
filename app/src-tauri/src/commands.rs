//! Commandes Tauri : l'API de docs/contracts.md §5, branchée sur `steno-core`.

use crate::AppState;
use steno_core::doctor::DoctorItem;
use steno_core::events::LiveState;
use steno_core::model::*;
use steno_core::settings::Settings;
use steno_core::wake::check::WakeCheck;
use steno_core::wake::variants::VariantCheck;
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
pub async fn start_meeting(st: State<'_, AppState>, title: Option<String>, in_person: Option<bool>) -> R<Meeting> {
    st.core.start_meeting_with(title, in_person.unwrap_or(false)).await.map_err(er)
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
pub fn get_meeting_context(st: State<'_, AppState>, id: String) -> R<steno_core::context::MeetingContext> {
    st.core.meeting_context(&id).map_err(er)
}

#[tauri::command]
pub fn set_meeting_context(
    st: State<'_, AppState>,
    id: String,
    text: String,
    template: Option<String>,
) -> R<steno_core::context::MeetingContext> {
    st.core.set_meeting_context(&id, &text, template.as_deref()).map_err(er)
}

/// Extraction du texte (PDF, classeurs) hors du fil principal.
#[tauri::command]
pub async fn add_context_file(
    st: State<'_, AppState>,
    id: String,
    name: String,
    data: String,
) -> R<steno_core::context::ContextFile> {
    let core = st.core.clone();
    tauri::async_runtime::spawn_blocking(move || core.add_context_file(&id, &name, &data))
        .await
        .map_err(|e| e.to_string())?
        .map_err(er)
}

#[tauri::command]
pub fn delete_context_file(st: State<'_, AppState>, file_id: i64) -> R<()> {
    st.core.delete_context_file(file_id).map_err(er)
}

#[tauri::command]
pub async fn upcoming(st: State<'_, AppState>, days: Option<i64>) -> R<steno_core::planning::Upcoming> {
    st.core.upcoming(days.unwrap_or(7)).await.map_err(er)
}

#[tauri::command]
pub fn plan_meeting(st: State<'_, AppState>, title: String, scheduled_at: String, event_id: Option<String>) -> R<Meeting> {
    st.core.plan_meeting(&title, &scheduled_at, event_id.as_deref()).map_err(er)
}

#[tauri::command]
pub async fn start_planned(st: State<'_, AppState>, id: String, in_person: Option<bool>) -> R<Meeting> {
    st.core.start_planned(&id, in_person.unwrap_or(false)).await.map_err(er)
}

#[tauri::command]
pub fn get_participants(st: State<'_, AppState>, id: String) -> R<Vec<steno_core::planning::Participant>> {
    st.core.participants(&id).map_err(er)
}

#[tauri::command]
pub fn set_participants(
    st: State<'_, AppState>,
    id: String,
    participants: Vec<steno_core::planning::Participant>,
) -> R<Vec<steno_core::planning::Participant>> {
    st.core.set_participants(&id, &participants).map_err(er)
}

#[tauri::command]
pub async fn prepare_questions(st: State<'_, AppState>, id: String) -> R<Vec<String>> {
    st.core.prepare_questions(&id).await.map_err(er)
}

#[tauri::command]
pub async fn prepare_apply(
    st: State<'_, AppState>,
    id: String,
    answers: Vec<steno_core::planning::PrepAnswer>,
) -> R<steno_core::context::MeetingContext> {
    st.core.prepare_apply(&id, &answers).await.map_err(er)
}

#[tauri::command]
pub fn get_update_status(st: State<'_, AppState>) -> steno_core::updates::UpdateStatus {
    st.core.update_status()
}

#[tauri::command]
pub async fn check_update(st: State<'_, AppState>) -> R<steno_core::updates::UpdateStatus> {
    st.core.check_update().await.map_err(er)
}

/// Asynchrone : l'installation est lancée sur le moteur de Tauri (une commande synchrone
/// tourne sur le fil principal, sans moteur asynchrone).
#[tauri::command]
pub async fn install_update(st: State<'_, AppState>) -> R<()> {
    st.core.install_update().map_err(er)
}

/// Redémarre l'app (nouvelle version installée par `make install`).
#[tauri::command]
pub fn restart_app(app: AppHandle, st: State<'_, AppState>) -> R<()> {
    if st.core.db().live_meeting().map_err(er)?.is_some() {
        return Err("une réunion est en cours : redémarrez après la réunion".into());
    }
    st.core.shutdown();
    app.restart();
}

#[tauri::command]
pub fn report_now(st: State<'_, AppState>, id: String) -> R<()> {
    st.core.report_now(&id).map_err(er)
}

#[tauri::command]
pub fn list_report_templates(st: State<'_, AppState>) -> Vec<steno_core::report::Template> {
    st.core.report_templates()
}

#[tauri::command]
pub fn correlate(st: State<'_, AppState>, id: String) -> R<()> {
    st.core.correlate(&id).map_err(er)
}

#[tauri::command]
pub fn export_meeting(st: State<'_, AppState>, id: String, format: Option<String>) -> R<String> {
    st.core.export_meeting_as(&id, format.as_deref().unwrap_or("markdown")).map_err(er)
}

/// Fenêtre « Enregistrer sous… » puis écriture de l'export ; chemin choisi, ou null si annulé.
#[tauri::command]
pub async fn save_export(app: AppHandle, st: State<'_, AppState>, id: String, format: String) -> R<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let name = st.core.export_file_name(&id, &format).map_err(er)?;
    let ext = steno_core::export::extension(&format).map_err(er)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let title = steno_core::i18n::tr(st.core.settings().lang(), "Enregistrer l'export", "Save export");
    let mut dialog = app.dialog().file().set_title(title).set_file_name(name);
    // Ouvert sur Téléchargements par défaut (macOS retient ensuite le dernier dossier choisi).
    if let Some(dl) = dirs::download_dir().filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dl);
    }
    dialog
        .add_filter(ext.to_uppercase(), &[ext])
        .save_file(move |f| {
            let _ = tx.send(f);
        });
    let Some(path) = rx.await.map_err(e)?.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    st.core.write_export(&id, &format, &path).map_err(er)?;
    Ok(Some(path.display().to_string()))
}

/// Impression de la fenêtre principale (« Enregistrer au format PDF » dans la fenêtre système).
#[tauri::command]
pub fn print_window(window: tauri::WebviewWindow) -> R<()> {
    window.print().map_err(e)
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
    crate::apply_theme(&app, &s.profile.theme);
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
    crate::apply_theme(&app, &s.profile.theme);
    Ok(s)
}

#[tauri::command]
pub async fn detect_agents(st: State<'_, AppState>) -> R<Vec<steno_core::agents::AgentInfo>> {
    Ok(st.core.detect_agents().await)
}

#[derive(Serialize)]
pub struct TestAgentResult {
    ok: bool,
    message: String,
    duration_ms: u128,
}

#[tauri::command]
pub fn list_audio_devices(st: State<'_, AppState>) -> steno_core::sidecar::AudioDevices {
    st.core.audio_devices()
}

#[tauri::command]
pub async fn list_models(st: State<'_, AppState>, provider: String) -> R<Vec<steno_core::agents::ModelInfo>> {
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
    Ok(steno_core::doctor::run(&st.core, false).await)
}

#[tauri::command]
pub fn delete_all_data(st: State<'_, AppState>) -> R<()> {
    st.core.delete_all_data().map_err(er)
}

// ------------------------------------------------------------------ permissions macOS

#[tauri::command]
pub fn get_permissions(st: State<'_, AppState>) -> R<steno_core::permissions::Permissions> {
    Ok(st.core.permissions())
}

#[tauri::command]
pub fn request_permission(st: State<'_, AppState>, kind: String) -> R<()> {
    st.core.request_permission(&kind).map_err(er)
}

#[tauri::command]
pub fn open_permission_settings(st: State<'_, AppState>, kind: String) -> R<()> {
    st.core.open_permission_settings(&kind).map_err(er)
}

#[tauri::command]
pub fn pause_meeting(st: State<'_, AppState>) -> R<()> {
    st.core.pause_meeting(true).map_err(er)
}

#[tauri::command]
pub fn resume_meeting(st: State<'_, AppState>) -> R<()> {
    st.core.pause_meeting(false).map_err(er)
}

#[tauri::command]
pub fn set_mic_muted(st: State<'_, AppState>, on: bool) {
    st.core.set_mic_muted(on);
}

#[tauri::command]
pub fn delete_segment(st: State<'_, AppState>, id: i64) -> R<()> {
    st.core.delete_segment(id).map_err(er)
}

#[tauri::command]
pub fn complete_onboarding(st: State<'_, AppState>) -> R<()> {
    st.core.complete_onboarding().map_err(er)
}

#[tauri::command]
pub fn log_ui_error(message: String) {
    tracing::error!(target: "ui", "{}", message.chars().take(4000).collect::<String>());
}
