//! Notifications système. Celles qui portent une route (réponse prête, compte rendu…) ouvrent
//! Sténo sur cette route au clic. `tauri-plugin-notification` ne gère pas le clic sur
//! macOS : on passe directement par `mac-notification-sys` (la bibliothèque qu'il utilise),
//! qui attend l'interaction dans un fil dédié.

use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::AppHandle;

/// Notifications en attente d'un clic ; au-delà, les suivantes ne sont plus cliquables
/// (une notification ignorée garde son fil d'attente).
static WAITING: AtomicUsize = AtomicUsize::new(0);
const MAX_WAITING: usize = 8;

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
    #[cfg(not(target_os = "macos"))]
    let _ = route;
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}
