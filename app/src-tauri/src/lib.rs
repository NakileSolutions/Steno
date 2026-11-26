//! App de bureau Scribe (Tauri 2) : barre de menu, fenêtre principale, notifications,
//! raccourcis globaux. Toute la logique est dans `scribe-core`.

mod commands;
mod shortcuts;
mod tray;

use scribe_core::core::CoreOptions;
use scribe_core::events::CoreEvent;
use scribe_core::live::SidecarSpec;
use scribe_core::{Core, Db};
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_notification::NotificationExt;

pub struct AppState {
    pub core: Core,
}

/// Sidecar audio : `SCRIBE_SIDECAR` (développement, ex. « scribe replay fichier.jsonl »),
/// sinon `scribe-audio` à côté de l'exécutable (Contents/MacOS dans Scribe.app).
fn sidecar_spec() -> SidecarSpec {
    if let Ok(cmd) = std::env::var("SCRIBE_SIDECAR") {
        let parts: Vec<String> = cmd.split_whitespace().map(String::from).collect();
        if let Some((p, a)) = parts.split_first() {
            return SidecarSpec { program: p.clone(), args: a.to_vec() };
        }
    }
    let next_to_exe = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("scribe-audio")))
        .filter(|p| p.exists());
    SidecarSpec {
        program: next_to_exe.map(|p| p.display().to_string()).unwrap_or_else(|| "scribe-audio".into()),
        args: vec![],
    }
}

fn init_logs() {
    let dir = scribe_core::paths::logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("scribe.log"));
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    match file {
        Ok(f) => {
            let _ = tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false).with_writer(std::sync::Mutex::new(f)).try_init();
        }
        Err(_) => {
            let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
        }
    }
}

fn play_feedback(core: &Core) {
    let mode = core.settings().voice.feedback;
    if mode == "none" {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        // « headset » : le son système sort par la sortie par défaut, c'est-à-dire le casque
        // pendant un appel ; il n'est jamais injecté dans l'audio de Teams.
        let _ = std::process::Command::new("afplay").args(["-v", "0.4", "/System/Library/Sounds/Tink.aiff"]).spawn();
    }
}

/// Relaie les événements du cœur vers la WebView, les notifications et l'icône.
fn forward_events(app: tauri::AppHandle, core: Core) {
    let mut rx = core.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            let ev = match rx.recv().await {
                Ok(ev) => ev,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            };
            match ev {
                CoreEvent::Live(st) => {
                    if let Some(tray) = app.tray_by_id("scribe") {
                        let tip = match (&st.meeting, st.state.as_str()) {
                            (Some(m), _) => format!("Scribe — ● écoute : {}", m.title.clone().unwrap_or_default()),
                            _ => "Scribe — en attente".to_string(),
                        };
                        let _ = tray.set_tooltip(Some(tip));
                        #[cfg(target_os = "macos")]
                        let _ = tray.set_title(if st.meeting.is_some() { Some("●") } else { None });
                    }
                    let mut st = st;
                    st.pending_requests = core.db().count_active_requests().unwrap_or(0);
                    let _ = app.emit("scribe://live", st);
                }
                CoreEvent::Segment(s) => {
                    let _ = app.emit("scribe://segment", s);
                }
                CoreEvent::Request(r) => {
                    let _ = app.emit("scribe://request", r);
                }
                CoreEvent::Report { meeting_id, version } => {
                    let _ = app.emit("scribe://report", serde_json::json!({ "meeting_id": meeting_id, "version": version }));
                }
                CoreEvent::MeetingsChanged => {
                    let _ = app.emit("scribe://meetings", ());
                }
                CoreEvent::Notify { title, body, route: _ } => {
                    let _ = app.notification().builder().title(title).body(body).show();
                }
                CoreEvent::CommandDetected => play_feedback(&core),
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logs();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app, None)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            scribe_core::paths::ensure_dirs()?;
            let db = Db::open_default()?;
            let core = Core::new(
                db,
                CoreOptions { hook_exe: std::env::current_exe().ok(), sidecar: Some(sidecar_spec()), agent_override: None },
            )?;
            let first_run = core.db().get_runtime("onboarded")?.is_none();
            app.manage(AppState { core: core.clone() });
            let c = core.clone();
            tauri::async_runtime::spawn(async move { c.start_background() });
            forward_events(app.handle().clone(), core.clone());
            tray::setup(app.handle())?;
            shortcuts::register(app.handle(), &core);
            if first_run {
                use tauri_plugin_autostart::ManagerExt;
                let _ = app.autolaunch().enable();
                core.db().set_runtime("onboarded", "1")?;
                tray::show_main(app.handle(), None);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.eval("location.hash = '#/onboarding'");
                }
            }
            Ok(())
        })
        .on_window_event(|win, ev| match ev {
            WindowEvent::CloseRequested { api, .. } => {
                // Fermer la fenêtre ne quitte pas l'app : elle vit dans la barre de menu.
                api.prevent_close();
                if win.label() == "main" {
                    tray::hide_main(win.app_handle());
                } else {
                    let _ = win.hide();
                }
            }
            WindowEvent::Focused(false) if win.label() == "tray" => {
                let _ = win.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::live_state,
            commands::start_meeting,
            commands::stop_meeting,
            commands::set_aside,
            commands::set_live_confidential,
            commands::ask,
            commands::ask_history,
            commands::cancel_request,
            commands::recent_requests,
            commands::list_meetings,
            commands::get_meeting,
            commands::get_transcript,
            commands::get_report_version,
            commands::rename_meeting,
            commands::set_confidential,
            commands::delete_meeting,
            commands::rename_speaker,
            commands::add_note,
            commands::update_note,
            commands::delete_note,
            commands::regenerate_report,
            commands::correlate,
            commands::export_meeting,
            commands::search,
            commands::list_actions,
            commands::set_action_status,
            commands::list_decisions,
            commands::get_settings,
            commands::save_settings,
            commands::export_settings,
            commands::import_settings,
            commands::detect_agents,
            commands::test_agent,
            commands::list_connectors,
            commands::refresh_connectors,
            commands::update_connector,
            commands::list_repos,
            commands::add_repo,
            commands::update_repo,
            commands::remove_repo,
            commands::pick_folder,
            commands::check_wake_word,
            commands::generate_variants,
            commands::check_variants,
            commands::wake_detect,
            commands::wake_test_record,
            commands::doctor,
            commands::delete_all_data,
            commands::log_ui_error,
        ])
        .build(tauri::generate_context!())
        .expect("erreur au démarrage de Scribe")
        .run(|app, ev| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = ev {
                // Pas de fenêtre ouverte ≠ quitter : seul « Quitter » ferme l'app.
                if code.is_none() {
                    api.prevent_exit();
                } else {
                    app.state::<AppState>().core.shutdown();
                }
            }
        });
}
