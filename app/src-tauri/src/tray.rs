//! Icône de la barre d'état macOS (en haut à droite, à côté de l'heure) et son menu natif :
//! démarrer / arrêter l'écoute, réunion confidentielle, demande rapide, ouvrir l'app, réglages.
//! Le menu suit l'état en direct (libellés mis à jour à chaque événement `Live`).

use scribe_core::events::LiveState;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Wry};

/// Éléments du menu dont le libellé ou l'état change.
pub struct TrayItems {
    status: MenuItem<Wry>,
    toggle: MenuItem<Wry>,
    confidential: CheckMenuItem<Wry>,
}

/// Libellés selon l'état (fonction pure, testée).
pub fn labels(st: &LiveState, shortcut: &str) -> (String, String) {
    let toggle_hint = if shortcut.is_empty() { String::new() } else { format!("  ({shortcut})") };
    match (&st.meeting, st.teams_call.active) {
        (Some(m), _) => (
            format!("● Écoute en cours — {}", m.title.clone().unwrap_or_else(|| "réunion".into())),
            format!("Arrêter l'écoute{toggle_hint}"),
        ),
        (None, true) => (
            format!("Appel Teams détecté : {}", st.teams_call.title.clone().unwrap_or_else(|| "sans titre".into())),
            format!("Démarrer l'écoute{toggle_hint}"),
        ),
        (None, false) => ("En attente — aucune écoute".into(), format!("Démarrer l'écoute{toggle_hint}")),
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let core = app.state::<crate::AppState>().core.clone();
    let shortcut = core.settings().audio.shortcuts.toggle;
    let (status_txt, toggle_txt) = labels(&core.live_state(), &shortcut);
    let status = MenuItem::with_id(app, "status", status_txt, false, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", toggle_txt, true, None::<&str>)?;
    let confidential = CheckMenuItem::with_id(app, "confidential", "Réunion confidentielle (aucune IA)", true, false, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick", "Demande rapide…", true, None::<&str>)?;
    let live = MenuItem::with_id(app, "live", "Voir la transcription en direct", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Ouvrir Scribe", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Réglages…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter Scribe", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[&status, &sep()?, &toggle, &confidential, &sep()?, &quick, &live, &open, &settings, &sep()?, &quit],
    )?;
    app.manage(TrayItems { status, toggle, confidential: confidential.clone() });

    TrayIconBuilder::with_id("scribe")
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray@2x.png"))?)
        .icon_as_template(true)
        .tooltip("Scribe")
        .menu(&menu)
        // Convention macOS : un clic sur l'icône ouvre le menu.
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, ev| {
            let core = app.state::<crate::AppState>().core.clone();
            match ev.id().as_ref() {
                "toggle" => {
                    tauri::async_runtime::spawn(async move {
                        let r = if core.live_state().meeting.is_some() {
                            core.stop_meeting().await
                        } else {
                            core.start_meeting(None).await.map(|_| ())
                        };
                        if let Err(e) = r {
                            tracing::warn!("{e:#}");
                        }
                    });
                }
                "confidential" => {
                    let checked = app.state::<TrayItems>().confidential.is_checked().unwrap_or(false);
                    if let Err(e) = core.set_live_confidential(checked) {
                        tracing::warn!("{e:#}");
                    }
                }
                "quick" => show_panel(app),
                "live" => show_main(app, Some("/live")),
                "open" => show_main(app, None),
                "settings" => show_main(app, Some("/settings")),
                "quit" => {
                    core.shutdown();
                    let a = app.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                        a.exit(0);
                    });
                }
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

/// Met à jour le menu et l'icône selon l'état en direct.
pub fn refresh(app: &AppHandle, st: &LiveState) {
    let core = app.state::<crate::AppState>().core.clone();
    let (status, toggle) = labels(st, &core.settings().audio.shortcuts.toggle);
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.status.set_text(status.clone());
        let _ = items.toggle.set_text(toggle);
        if let Some(m) = &st.meeting {
            let _ = items.confidential.set_checked(m.confidential);
        }
    }
    if let Some(tray) = app.tray_by_id("scribe") {
        let _ = tray.set_tooltip(Some(format!("Scribe — {status}")));
        // Point rouge à côté de l'icône pendant l'écoute.
        let _ = tray.set_title(if st.meeting.is_some() { Some("●") } else { None });
    }
}

/// Panneau rapide (demande, 5 dernières réponses), en haut à droite de l'écran.
pub fn show_panel(app: &AppHandle) {
    let Some(w) = app.get_webview_window("tray") else { return };
    let scale = w.scale_factor().unwrap_or(1.0);
    let size = w.outer_size().unwrap_or(tauri::PhysicalSize::new((360.0 * scale) as u32, (540.0 * scale) as u32));
    let monitor_w = w.current_monitor().ok().flatten().map(|m| m.size().width as f64).unwrap_or(1440.0 * scale);
    let _ = w.set_position(PhysicalPosition::new(monitor_w - size.width as f64 - 12.0 * scale, 30.0 * scale));
    let _ = w.show();
    let _ = w.set_focus();
}

/// Affiche la fenêtre principale (et l'icône dans le Dock tant qu'elle est ouverte).
pub fn show_main(app: &AppHandle, route: Option<&str>) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        if let Some(r) = route {
            let _ = app.emit_to("main", "scribe://navigate", serde_json::json!({ "route": r }));
        }
    }
}

pub fn hide_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

#[cfg(test)]
mod tests {
    use super::*;
    use scribe_core::events::TeamsCall;

    #[test]
    fn libelles_du_menu() {
        let mut st = LiveState::default();
        assert_eq!(labels(&st, "Ctrl+Alt+S"), ("En attente — aucune écoute".into(), "Démarrer l'écoute  (Ctrl+Alt+S)".into()));
        st.teams_call = TeamsCall { active: true, title: Some("Daily".into()) };
        assert_eq!(labels(&st, "").0, "Appel Teams détecté : Daily");
        st.meeting = Some(scribe_core::model::Meeting {
            id: "m".into(),
            title: Some("Daily".into()),
            title_source: None,
            started_at: String::new(),
            ended_at: None,
            status: "live".into(),
            confidential: false,
            agent_provider: None,
            agent_session_id: None,
            correlation: None,
        });
        let (s, t) = labels(&st, "");
        assert_eq!(s, "● Écoute en cours — Daily");
        assert_eq!(t, "Arrêter l'écoute");
    }
}
