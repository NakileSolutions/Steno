//! Raccourcis globaux (réglables) : démarrer/arrêter, aparté (maintenir), note rapide.
//!
//! Sous Wayland, les raccourcis globaux d'une app ne fonctionnent pas (le greffon passe par
//! X11). Les mêmes actions existent donc en ligne de commande, transmises à l'instance déjà
//! lancée (`tauri-plugin-single-instance`) : `steno-app --toggle`, `--aside` (bascule l'aparté),
//! `--note`, `--show`. L'utilisateur les lie à un raccourci personnalisé du bureau (GNOME :
//! Paramètres › Clavier › Raccourcis personnalisés). L'état des raccourcis globaux et la
//! commande à lier sont notés dans `runtime.app_shortcuts` (lus par `doctor` et l'interface).

use steno_core::permissions::AppShortcuts;
use steno_core::Core;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Action demandée en ligne de commande (ou par un raccourci du bureau).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliAction {
    /// Démarrer ou arrêter l'écoute.
    Toggle,
    /// Activer ou désactiver l'aparté.
    Aside,
    /// Panneau de note / demande rapide.
    Note,
    /// Afficher la fenêtre principale.
    Show,
}

/// Première action reconnue dans les arguments (le premier, nom du programme, est ignoré).
pub fn cli_action(args: &[String]) -> Option<CliAction> {
    args.iter().skip(1).find_map(|a| match a.as_str() {
        "--toggle" => Some(CliAction::Toggle),
        "--aside" => Some(CliAction::Aside),
        "--note" | "--quick-note" => Some(CliAction::Note),
        "--show" => Some(CliAction::Show),
        _ => None,
    })
}

/// Démarre l'écoute si rien n'écoute, sinon l'arrête.
pub fn toggle_listening(core: &Core) {
    let c = core.clone();
    tauri::async_runtime::spawn(async move {
        let live = c.live_state().meeting.is_some();
        let r = if live { c.stop_meeting().await } else { c.start_meeting(None).await.map(|_| ()) };
        if let Err(e) = r {
            tracing::warn!("démarrer/arrêter : {e:#}");
        }
    });
}

/// Exécute une action demandée en ligne de commande.
pub fn run_action(app: &AppHandle, core: &Core, action: CliAction) {
    tracing::info!("action en ligne de commande : {action:?}");
    match action {
        CliAction::Toggle => toggle_listening(core),
        CliAction::Aside => core.set_aside(!core.live_state().aside),
        CliAction::Note => crate::tray::show_panel(app),
        CliAction::Show => crate::tray::show_main(app, None),
    }
}

/// Ligne de commande de l'app à lier à un raccourci du bureau : l'AppImage elle-même si l'app
/// en vient, sinon l'exécutable courant (entre guillemets s'il contient une espace).
pub fn app_command() -> String {
    let exe = std::env::var("APPIMAGE")
        .ok()
        .filter(|p| !p.is_empty())
        .or_else(|| std::env::current_exe().ok().map(|p| p.display().to_string()))
        .unwrap_or_else(|| "steno-app".into());
    quote(&exe)
}

fn quote(path: &str) -> String {
    if path.chars().any(|c| c.is_whitespace() || c == '\'' || c == '"') {
        format!("'{}'", path.replace('\'', r"'\''"))
    } else {
        path.to_string()
    }
}

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

/// Enregistre les raccourcis globaux. Un échec (raccourci déjà pris, pas de serveur X) ne
/// bloque rien : les actions restent accessibles par le menu et la ligne de commande.
pub fn register(app: &AppHandle, core: &Core) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let sc = core.settings().audio.shortcuts;
    let mut ok = true;
    let c = core.clone();
    if let Err(err) = gs.on_shortcut(normalize(&sc.aside).as_str(), move |_app, _s, ev| {
        c.set_aside(ev.state() == ShortcutState::Pressed);
    }) {
        ok = false;
        tracing::warn!("raccourci d'aparté « {} » refusé : {err}", sc.aside);
    }
    let c = core.clone();
    if let Err(err) = gs.on_shortcut(normalize(&sc.toggle).as_str(), move |_app, _s, ev| {
        if ev.state() == ShortcutState::Pressed {
            toggle_listening(&c);
        }
    }) {
        ok = false;
        tracing::warn!("raccourci « {} » refusé : {err}", sc.toggle);
    }
    if let Err(err) = gs.on_shortcut(normalize(&sc.quick_note).as_str(), move |app, _s, ev| {
        if ev.state() == ShortcutState::Pressed {
            crate::tray::show_panel(app);
        }
    }) {
        ok = false;
        tracing::warn!("raccourci « {} » refusé : {err}", sc.quick_note);
    }
    let wayland = steno_core::permissions::session_type().as_deref() == Some("wayland");
    if wayland {
        tracing::info!(
            "session Wayland : raccourcis globaux indisponibles ; lier « {} --toggle | --aside | --note » à des raccourcis du bureau",
            app_command()
        );
    }
    let state = AppShortcuts { global: ok && !wayland, command: app_command() };
    if let Err(e) = steno_core::permissions::record_app_shortcuts(core.db(), &state) {
        tracing::warn!("état des raccourcis : {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format() {
        assert_eq!(normalize("Ctrl+Alt+Space"), "Control+Alt+Space");
        assert_eq!(normalize("ctrl+alt+s"), "Control+Alt+S");
        assert_eq!(normalize("Cmd+Shift+N"), "Super+Shift+N");
    }

    #[test]
    fn actions_en_ligne_de_commande() {
        let a = |v: &[&str]| cli_action(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(a(&["steno-app", "--toggle"]), Some(CliAction::Toggle));
        assert_eq!(a(&["steno-app", "--autostart", "--aside"]), Some(CliAction::Aside));
        assert_eq!(a(&["steno-app", "--note"]), Some(CliAction::Note));
        assert_eq!(a(&["steno-app", "--show"]), Some(CliAction::Show));
        assert_eq!(a(&["--toggle"]), None, "le nom du programme n'est pas une action");
        assert_eq!(a(&["steno-app", "--autostart"]), None);
        assert_eq!(a(&[]), None);
    }

    #[test]
    fn commande_entre_guillemets() {
        assert_eq!(quote("/usr/bin/steno-app"), "/usr/bin/steno-app");
        assert_eq!(quote("/home/a/Mes Apps/Steno.AppImage"), "'/home/a/Mes Apps/Steno.AppImage'");
        assert_eq!(quote("/opt/l'app/steno"), r"'/opt/l'\''app/steno'");
    }
}
