//! Raccourcis globaux (réglables) : démarrer/arrêter, aparté (maintenir), note rapide.

use scribe_core::Core;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// « Ctrl+Alt+Space » → format du greffon (« Control+Alt+Space »).
fn normalize(s: &str) -> String {
    s.split('+')
        .map(|p| match p.trim().to_lowercase().as_str() {
            "ctrl" | "control" | "⌃" => "Control".to_string(),
            "alt" | "option" | "opt" | "⌥" => "Alt".to_string(),
            "cmd" | "command" | "meta" | "super" | "⌘" => "Super".to_string(),
            "shift" | "⇧" => "Shift".to_string(),
            "espace" | "space" => "Space".to_string(),
            other if other.len() == 1 => other.to_uppercase(),
            _ => p.trim().to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

pub fn register(app: &AppHandle, core: &Core) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let sc = core.settings().audio.shortcuts;
    let c = core.clone();
    if let Err(err) = gs.on_shortcut(normalize(&sc.aside).as_str(), move |_app, _s, ev| {
        c.set_aside(ev.state() == ShortcutState::Pressed);
    }) {
        tracing::warn!("raccourci d'aparté « {} » refusé : {err}", sc.aside);
    }
    let c = core.clone();
    if let Err(err) = gs.on_shortcut(normalize(&sc.toggle).as_str(), move |_app, _s, ev| {
        if ev.state() != ShortcutState::Pressed {
            return;
        }
        let c = c.clone();
        tauri::async_runtime::spawn(async move {
            let live = c.live_state().meeting.is_some();
            let r = if live { c.stop_meeting().await } else { c.start_meeting(None).await.map(|_| ()) };
            if let Err(e) = r {
                tracing::warn!("démarrer/arrêter : {e:#}");
            }
        });
    }) {
        tracing::warn!("raccourci « {} » refusé : {err}", sc.toggle);
    }
    if let Err(err) = gs.on_shortcut(normalize(&sc.quick_note).as_str(), move |app, _s, ev| {
        if ev.state() == ShortcutState::Pressed {
            crate::tray::show_panel(app, None);
        }
    }) {
        tracing::warn!("raccourci « {} » refusé : {err}", sc.quick_note);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn format() {
        assert_eq!(super::normalize("Ctrl+Alt+Space"), "Control+Alt+Space");
        assert_eq!(super::normalize("ctrl+alt+s"), "Control+Alt+S");
        assert_eq!(super::normalize("Cmd+Shift+N"), "Super+Shift+N");
    }
}
