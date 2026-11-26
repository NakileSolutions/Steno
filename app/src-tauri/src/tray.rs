//! Icône de barre de menu, panneau rapide et fenêtre principale.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Rect};

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Ouvrir Scribe", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Démarrer / arrêter la réunion", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Réglages…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter Scribe", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &toggle, &settings, &PredefinedMenuItem::separator(app)?, &quit])?;
    TrayIconBuilder::with_id("scribe")
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray@2x.png"))?)
        .icon_as_template(true)
        .tooltip("Scribe")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "open" => show_main(app, None),
            "settings" => show_main(app, Some("/settings")),
            "toggle" => {
                let core = app.state::<crate::AppState>().core.clone();
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
            "quit" => {
                app.state::<crate::AppState>().core.shutdown();
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                    a.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = ev {
                let app = tray.app_handle();
                let visible = app.get_webview_window("tray").and_then(|w| w.is_visible().ok()).unwrap_or(false);
                if visible {
                    if let Some(w) = app.get_webview_window("tray") {
                        let _ = w.hide();
                    }
                } else {
                    show_panel(app, Some(rect));
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// Affiche le panneau sous l'icône (ou en haut à droite de l'écran).
pub fn show_panel(app: &AppHandle, rect: Option<Rect>) {
    let Some(w) = app.get_webview_window("tray") else { return };
    let scale = w.scale_factor().unwrap_or(1.0);
    let size = w.outer_size().unwrap_or(tauri::PhysicalSize::new((360.0 * scale) as u32, (540.0 * scale) as u32));
    let pos = match rect {
        Some(r) => {
            let p = r.position.to_physical::<f64>(scale);
            let s = r.size.to_physical::<f64>(scale);
            PhysicalPosition::new((p.x + s.width / 2.0 - size.width as f64 / 2.0).max(0.0), p.y + s.height + 4.0)
        }
        None => {
            let m = w.current_monitor().ok().flatten();
            let mw = m.as_ref().map(|m| m.size().width as f64).unwrap_or(1440.0 * scale);
            PhysicalPosition::new(mw - size.width as f64 - 16.0 * scale, 32.0 * scale)
        }
    };
    let _ = w.set_position(pos);
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
