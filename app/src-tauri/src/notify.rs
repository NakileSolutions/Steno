//! Notifications système. Celles qui portent une route (réponse prête, compte rendu…) ouvrent
//! Sténo sur cette route au clic.
//!
//! - macOS : `tauri-plugin-notification` ne gérait pas le clic ; on passe directement par
//!   `mac-notification-sys` (la bibliothèque qu'il utilise), qui attend l'interaction dans un fil
//!   dédié.
//! - Linux : le greffon suit les notifications sur D-Bus (serveur freedesktop) et signale le clic
//!   (`tap`) aux gestionnaires enregistrés par [`init`] ; la route voyage dans `extra`.

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::AppHandle;

/// Notifications en attente d'un clic ; au-delà, les suivantes ne sont plus cliquables
/// (une notification ignorée garde son fil d'attente).
#[cfg(target_os = "macos")]
static WAITING: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_os = "macos")]
const MAX_WAITING: usize = 8;

/// Gestionnaire du clic sur les notifications (hors macOS) : ouvre Sténo, sur la route de la
/// notification s'il y en a une.
pub fn init(app: &AppHandle) {
    #[cfg(not(target_os = "macos"))]
    {
        use tauri_plugin_notification::NotificationExt;
        let handle = app.clone();
        let _ = app.notification().on_action(move |performed| {
            if performed.action_id() != "tap" {
                return;
            }
            let route = performed
                .notification()
                .and_then(|n| n.extra().get("route"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let h = handle.clone();
            let _ = handle.run_on_main_thread(move || crate::tray::show_main(&h, route.as_deref()));
        });
    }
    #[cfg(target_os = "macos")]
    let _ = app;
}

pub fn show(app: &AppHandle, title: &str, body: &str, route: Option<&str>) {
    #[cfg(target_os = "macos")]
    if let Some(route) = route {
        if WAITING.fetch_add(1, Ordering::SeqCst) < MAX_WAITING {
            let _ = mac_notification_sys::set_application(&app.config().identifier);
            let (app, title, body, route) = (app.clone(), title.to_string(), body.to_string(), route.to_string());
            std::thread::spawn(move || {
                let mut n = mac_notification_sys::Notification::new();
                n.title(&title).message(&body).wait_for_click(true);
                let clicked = matches!(n.send(), Ok(mac_notification_sys::NotificationResponse::Click));
                WAITING.fetch_sub(1, Ordering::SeqCst);
                if clicked {
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || crate::tray::show_main(&handle, Some(&route)));
                }
            });
            return;
        }
        WAITING.fetch_sub(1, Ordering::SeqCst);
    }
    use tauri_plugin_notification::NotificationExt;
    let n = app.notification().builder().title(title).body(body);
    #[cfg(not(target_os = "macos"))]
    let n = match route {
        Some(route) => n.extra("route", route),
        None => n,
    };
    #[cfg(target_os = "macos")]
    let _ = route;
    if let Err(e) = n.show() {
        tracing::warn!("notification : {e}");
    }
}
