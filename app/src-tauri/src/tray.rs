//! Icône de la barre d'état macOS (en haut à droite, à côté de l'heure) et son menu natif :
//! démarrer / arrêter l'écoute, réunion confidentielle, demande rapide, ouvrir l'app, réglages.
//! Le menu suit l'état en direct (libellés mis à jour à chaque événement `Live`).

use steno_core::events::LiveState;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Wry};

/// Éléments du menu dont le libellé ou l'état change.
pub struct TrayItems {
    status: MenuItem<Wry>,
    toggle: MenuItem<Wry>,
    /// « Démarrer une réunion en présentiel » (masqué pendant l'écoute).
    room: MenuItem<Wry>,
    confidential: CheckMenuItem<Wry>,
    mute: CheckMenuItem<Wry>,
    /// « Dernières réponses » : les 5 dernières demandes terminées.
    answers: Submenu<Wry>,
}

/// Nombre de réponses listées dans le menu.
const RECENT_ANSWERS: i64 = 5;

/// Route de l'interface qui affiche une demande (même règle que la notification).
pub fn request_route(r: &steno_core::model::Request) -> String {
    match &r.meeting_id {
        Some(m) => format!("/meetings/{m}?tab=requests&request={}", r.id),
        None => format!("/search?request={}", r.id),
    }
}

/// Libellé court d'une réponse dans le menu.
fn answer_label(r: &steno_core::model::Request) -> String {
    let base = r.summary.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| r.query.clone());
    let icon = match r.status.as_str() {
        "done" => "",
        "error" => "⚠️ ",
        "cancelled" => "⊘ ",
        _ => "… ",
    };
    let one_line = base.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = one_line.chars().take(60).collect();
    format!("{icon}{short}{}", if one_line.chars().count() > 60 { "…" } else { "" })
}

/// Reconstruit le sous-menu des dernières réponses.
pub fn refresh_answers(app: &AppHandle) {
    let Some(items) = app.try_state::<TrayItems>() else { return };
    let core = app.state::<crate::AppState>().core.clone();
    let recent = core.recent_requests(RECENT_ANSWERS).unwrap_or_default();
    if let Ok(old) = items.answers.items() {
        for it in old {
            let _ = items.answers.remove(&it);
        }
    }
    if recent.is_empty() {
        if let Ok(empty) = MenuItem::with_id(app, "answers-empty", l(core_lang(app), "Aucune réponse pour l'instant", "No answers yet"), false, None::<&str>) {
            let _ = items.answers.append(&empty);
        }
        return;
    }
    for r in &recent {
        if let Ok(it) = MenuItem::with_id(app, format!("req:{}", r.id), answer_label(r), true, None::<&str>) {
            let _ = items.answers.append(&it);
        }
    }
}

/// Texte du menu dans la langue des réglages.
fn l(lang: &str, fr: &str, en: &str) -> String {
    steno_core::i18n::tr(lang, fr, en)
}

/// Libellés selon l'état, en français (fonction pure, testée).
#[cfg(test)]
pub fn labels(st: &LiveState, shortcut: &str) -> (String, String) {
    labels_in(st, shortcut, "fr")
}

/// Libellés selon l'état, dans la langue donnée.
pub fn labels_in(st: &LiveState, shortcut: &str, lang: &str) -> (String, String) {
    let toggle_hint = if shortcut.is_empty() { String::new() } else { format!("  ({shortcut})") };
    let start = format!("{}{toggle_hint}", l(lang, "Démarrer l'écoute", "Start listening"));
    match (&st.meeting, st.call.active) {
        (Some(m), _) => (
            format!(
                "{} — {}",
                l(lang, "● Écoute en cours", "● Listening"),
                m.title.clone().unwrap_or_else(|| l(lang, "réunion", "meeting"))
            ),
            format!("{}{toggle_hint}", l(lang, "Arrêter l'écoute", "Stop listening")),
        ),
        (None, true) => (
            format!("{} : {}", call_label(st, lang), st.call.title.clone().unwrap_or_else(|| l(lang, "sans titre", "untitled"))),
            start,
        ),
        (None, false) => (l(lang, "En attente — aucune écoute", "Idle — not listening"), start),
    }
}

/// « Appel Teams détecté », « Appel Slack détecté », « Appel détecté dans le navigateur ».
fn call_label(st: &LiveState, lang: &str) -> String {
    let en = lang == "en";
    match (st.call.app.as_deref(), st.call.app_name.as_deref()) {
        (Some("browser"), _) => l(lang, "Appel détecté dans le navigateur", "Call detected in the browser"),
        (Some("teams"), _) => l(lang, "Appel Teams détecté", "Teams call detected"),
        (_, Some(name)) => {
            let name = name.split(" (").next().unwrap_or(name);
            if en { format!("{name} call detected") } else { format!("Appel {name} détecté") }
        }
        _ => l(lang, "Appel détecté", "Call detected"),
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let core = app.state::<crate::AppState>().core.clone();
    let shortcut = core.settings().audio.shortcuts.toggle;
    let lang = core.settings().lang();
    let (status_txt, toggle_txt) = labels_in(&core.live_state(), &shortcut, lang);
    let status = MenuItem::with_id(app, "status", status_txt, false, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", toggle_txt, true, None::<&str>)?;
    let confidential = CheckMenuItem::with_id(app, "confidential", l(lang, "Réunion confidentielle (aucune IA)", "Confidential meeting (no AI)"), true, false, None::<&str>)?;
    let room = MenuItem::with_id(app, "room", l(lang, "Démarrer une réunion en présentiel", "Start an in-person meeting"), true, None::<&str>)?;
    let mute = CheckMenuItem::with_id(app, "mute", l(lang, "Couper mon micro dans Sténo", "Mute my mic in Sténo"), false, false, None::<&str>)?;
    let answers = Submenu::with_id(app, "answers", l(lang, "Dernières réponses", "Latest answers"), true)?;
    let quick = MenuItem::with_id(app, "quick", l(lang, "Demande rapide…", "Quick request…"), true, None::<&str>)?;
    let live = MenuItem::with_id(app, "live", l(lang, "Voir la transcription en direct", "Show live transcript"), true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", l(lang, "Ouvrir Sténo", "Open Sténo"), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", l(lang, "Réglages…", "Settings…"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", l(lang, "Quitter Sténo", "Quit Sténo"), true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[&status, &sep()?, &toggle, &room, &mute, &confidential, &sep()?, &answers, &quick, &live, &open, &settings, &sep()?, &quit],
    )?;
    app.manage(TrayItems { status, toggle, room, confidential: confidential.clone(), mute: mute.clone(), answers });
    refresh_answers(app);

    TrayIconBuilder::with_id("steno")
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray@2x.png"))?)
        .icon_as_template(true)
        .tooltip("Sténo")
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
                "room" => {
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = core.start_meeting_with(None, true).await {
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
                "mute" => {
                    let checked = app.state::<TrayItems>().mute.is_checked().unwrap_or(false);
                    core.set_mic_muted(checked);
                }
                "quick" => show_panel(app),
                id if id.starts_with("req:") => {
                    let rid = &id[4..];
                    if let Ok(Some(r)) = core.db().get_request(rid) {
                        show_main(app, Some(&request_route(&r)));
                    }
                }
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
    let (status, toggle) = labels_in(st, &core.settings().audio.shortcuts.toggle, core.settings().lang());
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.status.set_text(status.clone());
        let _ = items.toggle.set_text(toggle);
        if let Some(m) = &st.meeting {
            let _ = items.confidential.set_checked(m.confidential);
        }
        let _ = items.room.set_enabled(st.meeting.is_none());
        // Couper le micro n'a de sens que pendant l'écoute.
        let _ = items.mute.set_enabled(st.meeting.is_some());
        let _ = items.mute.set_checked(st.mic_muted);
    }
    if let Some(tray) = app.tray_by_id("steno") {
        let _ = tray.set_tooltip(Some(format!("Sténo — {status}")));
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
            let _ = app.emit_to("main", "steno://navigate", serde_json::json!({ "route": r }));
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
    use steno_core::events::CallState;

    #[test]
    fn libelles_du_menu() {
        let mut st = LiveState::default();
        assert_eq!(labels(&st, "Ctrl+Alt+S"), ("En attente — aucune écoute".into(), "Démarrer l'écoute  (Ctrl+Alt+S)".into()));
        st.call = CallState { active: true, app: Some("teams".into()), app_name: Some("Microsoft Teams".into()), title: Some("Daily".into()) };
        assert_eq!(labels(&st, "").0, "Appel Teams détecté : Daily");
        st.call = CallState { active: true, app: Some("slack".into()), app_name: Some("Slack".into()), title: Some("Huddle #flotte".into()) };
        assert_eq!(labels(&st, "").0, "Appel Slack détecté : Huddle #flotte");
        st.call.app = Some("browser".into());
        assert_eq!(labels(&st, "").0, "Appel détecté dans le navigateur : Huddle #flotte");
        st.meeting = Some(steno_core::model::Meeting {
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
            call_app: None,
        });
        let (s, t) = labels(&st, "");
        assert_eq!(s, "● Écoute en cours — Daily");
        assert_eq!(t, "Arrêter l'écoute");
    }
}

fn core_lang(app: &AppHandle) -> &'static str {
    app.try_state::<crate::AppState>().map(|s| s.core.settings().lang()).unwrap_or("fr")
}
