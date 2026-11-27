//! App de bureau Sténo (Tauri 2) : barre de menu, fenêtre principale, notifications,
//! raccourcis globaux. Toute la logique est dans `steno-core`.

mod commands;
mod shortcuts;
mod notify;
mod tray;

use steno_core::core::CoreOptions;
use steno_core::events::CoreEvent;
use steno_core::live::SidecarSpec;
use steno_core::{Core, Db};
use tauri::{Emitter, Manager, WindowEvent};

pub struct AppState {
    pub core: Core,
}

/// Sidecar audio : `STENO_SIDECAR` (développement, ex. « steno replay fichier.jsonl »),
/// sinon `steno-audio` à côté de l'exécutable (Contents/MacOS dans Steno.app).
fn sidecar_spec() -> SidecarSpec {
    if let Ok(cmd) = std::env::var("STENO_SIDECAR") {
        let parts: Vec<String> = cmd.split_whitespace().map(String::from).collect();
        if let Some((p, a)) = parts.split_first() {
            return SidecarSpec { program: p.clone(), args: a.to_vec() };
        }
    }
    let next_to_exe = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("steno-audio")))
        .filter(|p| p.exists());
    SidecarSpec {
        program: next_to_exe.map(|p| p.display().to_string()).unwrap_or_else(|| "steno-audio".into()),
        args: vec![],
    }
}

/// Lancée par le Finder, le Dock ou l'ouverture de session, l'app hérite du PATH minimal de
/// launchd (`/usr/bin:/bin:…`) : les CLI d'agent (`~/.local/bin/claude`, Homebrew, nvm…) y
/// sont introuvables. On reprend le PATH du shell de connexion de l'utilisateur, complété
/// des dossiers d'installation courants.
fn complete_path() {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut dirs: Vec<String> = std::env::var("PATH").unwrap_or_default().split(':').map(String::from).collect();
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".into());
    let marker = "__STENO_PATH__";
    let login_path = std::process::Command::new(&shell)
        .args(["-ilc", &format!("printf '{marker}%s{marker}' \"$PATH\"")])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .and_then(|o| {
            let out = String::from_utf8_lossy(&o.stdout).to_string();
            let start = out.find(marker)? + marker.len();
            let end = start + out[start..].find(marker)?;
            Some(out[start..end].to_string())
        });
    let extra = [
        format!("{home}/.local/bin"),
        "/opt/homebrew/bin".into(),
        // Rust installé par Homebrew (rustup, formule « keg-only ») : utile aux mises à jour.
        "/opt/homebrew/opt/rustup/bin".into(),
        "/opt/homebrew/sbin".into(),
        "/usr/local/bin".into(),
        format!("{home}/.npm-global/bin"),
        format!("{home}/.bun/bin"),
        format!("{home}/.volta/bin"),
        format!("{home}/.cargo/bin"),
    ];
    for d in login_path.iter().flat_map(|p| p.split(':').map(String::from)).chain(extra) {
        if !d.is_empty() && !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    std::env::set_var("PATH", dirs.join(":"));
    tracing::info!("PATH complété ({} dossiers)", dirs.len());
}

fn init_logs() {
    // Avant toute création de dossier : reprise des données de l'ancien nom (« Scribe »).
    let _ = steno_core::paths::migrate_legacy();
    let dir = steno_core::paths::logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("steno.log"));
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
            match &ev {
                CoreEvent::Live(st) => tray::refresh(&app, st),
                CoreEvent::Notify { title, body, route } => notify::show(&app, title, body, route.as_deref()),
                CoreEvent::Request(r) if matches!(r.status.as_str(), "done" | "error" | "cancelled") => {
                    tray::refresh_answers(&app)
                }
                // Le sidecar joue le signal sur la sortie choisie ; sinon, repli sur le son système.
                CoreEvent::CommandDetected { played: false } => play_feedback(&core),
                _ => {}
            }
            if let Some((name, mut payload)) = ev.to_wire() {
                if name == "steno://live" {
                    payload["pending_requests"] = core.db().count_active_requests().unwrap_or(0).into();
                }
                let _ = app.emit(name, payload);
            }
        }
    });
}

/// Argument passé par l'agent de lancement à l'ouverture de session.
const AUTOSTART_ARG: &str = "--autostart";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logs();
    complete_path();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app, None)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![AUTOSTART_ARG])))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            steno_core::paths::ensure_dirs()?;
            let db = Db::open_default()?;
            let core = Core::new(
                db,
                CoreOptions { hook_exe: std::env::current_exe().ok(), sidecar: Some(sidecar_spec()), agent_override: None },
            )?;
            // L'assistant s'ouvre tant qu'il n'a pas été terminé ou passé (`complete_onboarding`),
            // même si l'app a été relancée entre-temps.
            let first_run = !core.onboarded()?;
            commands::init(&core);
            app.manage(AppState { core: core.clone() });
            let c = core.clone();
            tauri::async_runtime::spawn(async move { c.start_background() });
            forward_events(app.handle().clone(), core.clone());
            tray::setup(app.handle())?;
            shortcuts::register(app.handle(), &core);
            apply_theme(app.handle(), &core.settings().profile.theme);
            if first_run {
                use tauri_plugin_autostart::ManagerExt;
                if !app.autolaunch().is_enabled().unwrap_or(false) {
                    let _ = app.autolaunch().enable();
                }
                tray::show_main(app.handle(), None);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.eval("location.hash = '#/onboarding'");
                }
            } else {
                // L'app a pu être déplacée (ex. vers /Applications) : on réécrit l'agent de
                // lancement avec le chemin et les arguments actuels.
                use tauri_plugin_autostart::ManagerExt;
                match app.autolaunch().is_enabled() {
                    Ok(true) => {
                        if let Err(e) = app.autolaunch().enable() {
                            tracing::warn!("mise à jour du lancement à l'ouverture de session : {e}");
                        }
                    }
                    Ok(false) => {}
                    Err(e) => tracing::warn!("état du lancement à l'ouverture de session : {e}"),
                }
            }
            if !first_run && !std::env::args().any(|a| a == AUTOSTART_ARG) {
                // Lancement manuel : on montre la fenêtre. À l'ouverture de session, l'app
                // reste discrète dans la barre de menu.
                tray::show_main(app.handle(), None);
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
            commands::get_meeting_context,
            commands::set_meeting_context,
            commands::add_context_file,
            commands::delete_context_file,
            commands::list_report_templates,
            commands::template_from_example,
            commands::steno_skill_status,
            commands::install_steno_skill,
            commands::report_now,
            commands::get_update_status,
            commands::upcoming,
            commands::plan_meeting,
            commands::start_planned,
            commands::get_participants,
            commands::set_participants,
            commands::prepare_questions,
            commands::prepare_apply,
            commands::check_update,
            commands::install_update,
            commands::restart_app,
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
            commands::list_audio_devices,
            commands::list_models,
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
            commands::get_permissions,
            commands::request_permission,
            commands::open_permission_settings,
            commands::complete_onboarding,
            commands::delete_segment,
            commands::set_mic_muted,
            commands::save_export,
            commands::print_window,
            commands::pause_meeting,
            commands::resume_meeting,
        ])
        .build(tauri::generate_context!())
        .expect("erreur au démarrage de Sténo")
        .run(|app, ev| match ev {
            tauri::RunEvent::ExitRequested { api, code, .. } => {
                // Pas de fenêtre ouverte ≠ quitter : seul « Quitter » ferme l'app.
                if code.is_none() {
                    api.prevent_exit();
                } else {
                    app.state::<AppState>().core.shutdown();
                }
            }
            // Clic sur l'icône dans le Finder, le Launchpad ou Spotlight alors que l'app tourne
            // déjà (dans la barre de menu) : macOS envoie « reopen », pas un second lancement.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::show_main(app, None),
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    /// L'app expose exactement les commandes du contrat (même liste que `steno serve`).
    #[test]
    fn commandes_tauri_conformes_au_contrat() {
        let src = include_str!("lib.rs");
        let start = src.find("generate_handler![").unwrap();
        let end = start + src[start..].find(']').unwrap();
        let mut registered: Vec<&str> = src[start..end]
            .split("commands::")
            .skip(1)
            .map(|s| s.trim().trim_end_matches(',').trim())
            .collect();
        registered.sort();
        let mut contract: Vec<&str> = steno_core::api::COMMANDS.to_vec();
        contract.sort();
        assert_eq!(registered, contract);
    }
}

/// Apparence de toutes les fenêtres : `light`, `dark`, ou `auto` (suit macOS). Le thème
/// natif de la fenêtre pilote aussi `prefers-color-scheme` dans la vue web.
pub fn apply_theme(app: &tauri::AppHandle, theme: &str) {
    let t = match theme {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    };
    app.set_theme(t);
}
