//! Icône de la barre d'état (macOS : en haut à droite, à côté de l'heure ; Linux : zone
//! d'indicateurs, via libayatana-appindicator) et son menu natif : démarrer / arrêter l'écoute,
//! réunion en présentiel, pause, aparté, micro coupé, réunion confidentielle, demande rapide,
//! ouvrir l'app, réglages. Le menu suit l'état en direct (libellés mis à jour à chaque événement
//! `Live`).
//!
//! Sous Linux, le clic sur l'icône n'est pas remonté à l'app (seul le menu s'ouvre) : tout ce
//! que propose le panneau rapide (`#/tray`) doit donc aussi être dans le menu.

use steno_core::events::LiveState;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Wry};

/// Éléments du menu dont le libellé ou l'état change.
pub struct TrayItems {
    status: MenuItem<Wry>,
    toggle: MenuItem<Wry>,
    /// « Démarrer une réunion en présentiel » (masqué pendant l'écoute).
    room: MenuItem<Wry>,
    /// « Mettre l'écoute en pause » / « Reprendre l'écoute ».
    pause: MenuItem<Wry>,
    /// Aparté (rien de ce que vous dites n'entre dans le compte rendu).
    aside: CheckMenuItem<Wry>,
    confidential: CheckMenuItem<Wry>,
    mute: CheckMenuItem<Wry>,
    /// « Dernières réponses » : les 5 dernières demandes terminées.
    answers: Submenu<Wry>,
    /// Dernier état affiché : le menu n'est réécrit que s'il change (l'état en direct arrive
    /// jusqu'à 4 fois par seconde avec les niveaux ; sous Linux, chaque modification du menu
    /// repasse par D-Bus).
    shown: std::sync::Mutex<String>,
    quick: MenuItem<Wry>,
    live: MenuItem<Wry>,
    open: MenuItem<Wry>,
    settings: MenuItem<Wry>,
    quit: MenuItem<Wry>,
    /// A meeting was live at the last refresh: when it ends, the "Confidential meeting"
    /// checkbox is cleared (the choice only applies to one meeting).
    was_live: AtomicBool,
}

/// Fixed menu labels, in the settings language.
struct StaticLabels {
    aside: String,
    confidential: String,
    room: String,
    mute: String,
    answers: String,
    quick: String,
    live: String,
    open: String,
    settings: String,
    quit: String,
}

fn static_labels(lang: &str) -> StaticLabels {
    StaticLabels {
        aside: l(lang, "Aparté (hors compte rendu)", "Aside (left out of the summary)"),
        confidential: l(lang, "Réunion confidentielle (aucune IA)", "Confidential meeting (no AI)"),
        room: l(lang, "Démarrer une réunion en présentiel", "Start an in-person meeting"),
        mute: l(lang, "Couper mon micro dans Sténo", "Mute my mic in Sténo"),
        answers: l(lang, "Dernières réponses", "Latest answers"),
        quick: l(lang, "Demande rapide…", "Quick request…"),
        live: l(lang, "Voir la transcription en direct", "Show live transcript"),
        open: l(lang, "Ouvrir Sténo", "Open Sténo"),
        settings: l(lang, "Réglages…", "Settings…"),
        quit: l(lang, "Quitter Sténo", "Quit Sténo"),
    }
}

/// Settings saved (language, shortcut): rewrites every menu label.
pub fn relabel(app: &AppHandle) {
    let Some(items) = app.try_state::<TrayItems>() else { return };
    let t = static_labels(core_lang(app));
    let _ = items.aside.set_text(t.aside);
    let _ = items.confidential.set_text(t.confidential);
    let _ = items.room.set_text(t.room);
    let _ = items.mute.set_text(t.mute);
    let _ = items.answers.set_text(t.answers);
    let _ = items.quick.set_text(t.quick);
    let _ = items.live.set_text(t.live);
    let _ = items.open.set_text(t.open);
    let _ = items.settings.set_text(t.settings);
    let _ = items.quit.set_text(t.quit);
    // State-dependent labels (status, pause): force the menu to be rewritten.
    items.shown.lock().unwrap().clear();
    let core = app.state::<crate::AppState>().core.clone();
    refresh(app, &core.live_state());
    refresh_answers(app);
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

/// Libellé de l'élément pause / reprise selon l'état.
pub fn pause_label(st: &LiveState, lang: &str) -> String {
    if st.state == "paused" {
        l(lang, "Reprendre l'écoute", "Resume listening")
    } else {
        l(lang, "Mettre l'écoute en pause", "Pause listening")
    }
}

/// Raccourci affiché à côté de « Démarrer l'écoute » : rien sous Wayland, où les raccourcis
/// globaux de l'app ne fonctionnent pas.
fn toggle_hint(core: &steno_core::Core) -> String {
    if steno_core::permissions::session_type().as_deref() == Some("wayland") {
        String::new()
    } else {
        core.settings().audio.shortcuts.toggle
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
    let shortcut = toggle_hint(&core);
    let lang = core.settings().lang();
    let st = core.live_state();
    let (status_txt, toggle_txt) = labels_in(&st, &shortcut, lang);
    let t = static_labels(lang);
    let status = MenuItem::with_id(app, "status", status_txt, false, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", toggle_txt, true, None::<&str>)?;
    let listening = st.meeting.is_some();
    let pause = MenuItem::with_id(app, "pause", pause_label(&st, lang), listening, None::<&str>)?;
    let aside = CheckMenuItem::with_id(app, "aside", t.aside, listening, st.aside, None::<&str>)?;
    let confidential = CheckMenuItem::with_id(app, "confidential", t.confidential, true, false, None::<&str>)?;
    let room = MenuItem::with_id(app, "room", t.room, true, None::<&str>)?;
    let mute = CheckMenuItem::with_id(app, "mute", t.mute, false, false, None::<&str>)?;
    let answers = Submenu::with_id(app, "answers", t.answers, true)?;
    let quick = MenuItem::with_id(app, "quick", t.quick, true, None::<&str>)?;
    let live = MenuItem::with_id(app, "live", t.live, true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", t.open, true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", t.settings, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", t.quit, true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &sep()?,
            &toggle,
            &room,
            &pause,
            &aside,
            &mute,
            &confidential,
            &sep()?,
            &answers,
            &quick,
            &live,
            &open,
            &settings,
            &sep()?,
            &quit,
        ],
    )?;
    app.manage(TrayItems {
        status,
        toggle,
        room,
        pause,
        aside,
        confidential,
        mute,
        answers,
        quick,
        live,
        open,
        settings,
        quit,
        shown: std::sync::Mutex::new(String::new()),
        was_live: AtomicBool::new(false),
    });
    refresh_answers(app);

    // macOS : icône « modèle » (noire, recolorée par le système). Linux : la barre du haut de
    // GNOME est sombre, en thème clair comme en thème sombre : icône blanche.
    #[cfg(target_os = "macos")]
    let icon = include_bytes!("../icons/tray@2x.png").as_slice();
    #[cfg(not(target_os = "macos"))]
    let icon = include_bytes!("../icons/tray-light@2x.png").as_slice();
    TrayIconBuilder::with_id("steno")
        .icon(tauri::image::Image::from_bytes(icon)?)
        .icon_as_template(true)
        .tooltip("Sténo")
        .menu(&menu)
        // Convention macOS : un clic sur l'icône ouvre le menu (sous Linux, c'est toujours le cas).
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
                "pause" => {
                    let paused = core.live_state().state == "paused";
                    if let Err(e) = core.pause_meeting(!paused) {
                        tracing::warn!("{e:#}");
                    }
                }
                "aside" => {
                    let checked = app.state::<TrayItems>().aside.is_checked().unwrap_or(false);
                    core.set_aside(checked);
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
                "quit" => crate::quit(app),
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

/// Met à jour le menu et l'icône selon l'état en direct.
pub fn refresh(app: &AppHandle, st: &LiveState) {
    let core = app.state::<crate::AppState>().core.clone();
    let lang = core.settings().lang();
    let (status, toggle) = labels_in(st, &toggle_hint(&core), lang);
    let listening = st.meeting.is_some();
    let confidential = st.meeting.as_ref().map(|m| m.confidential);
    let key = format!("{status}|{toggle}|{}|{listening}|{}|{}|{confidential:?}", st.state, st.aside, st.mic_muted);
    if let Some(items) = app.try_state::<TrayItems>() {
        let mut shown = items.shown.lock().unwrap();
        if *shown == key {
            return;
        }
        *shown = key;
        let _ = items.status.set_text(status.clone());
        let _ = items.toggle.set_text(toggle);
        let _ = items.pause.set_text(pause_label(st, lang));
        let _ = items.pause.set_enabled(st.meeting.is_some());
        let _ = items.aside.set_enabled(st.meeting.is_some());
        let _ = items.aside.set_checked(st.aside);
        let was_live = items.was_live.swap(st.meeting.is_some(), Ordering::SeqCst);
        match &st.meeting {
            Some(m) => {
                let _ = items.confidential.set_checked(m.confidential);
            }
            // Meeting ended: the "confidential" choice was used up by that meeting. Outside
            // a meeting, the checkbox stays free to set up the next one.
            None if was_live => {
                let _ = items.confidential.set_checked(false);
            }
            None => {}
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
    // Monitor under the pointer (the menu bar that was clicked), else the window's monitor.
    let monitor = w
        .cursor_position()
        .ok()
        .and_then(|p| w.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| w.current_monitor().ok().flatten());
    let (origin_x, origin_y, monitor_w) = monitor
        .map(|m| (m.position().x as f64, m.position().y as f64, m.size().width as f64))
        .unwrap_or((0.0, 0.0, 1440.0 * scale));
    let (x, y) = panel_position(origin_x, origin_y, monitor_w, size.width as f64, scale);
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.show();
    let _ = w.set_focus();
}

/// Top-right corner of the monitor (origin `origin_x`, `origin_y`, physical pixels), below the
/// menu bar. On a secondary monitor the origin is not (0, 0).
fn panel_position(origin_x: f64, origin_y: f64, monitor_w: f64, panel_w: f64, scale: f64) -> (f64, f64) {
    (origin_x + monitor_w - panel_w - 12.0 * scale, origin_y + 30.0 * scale)
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

fn core_lang(app: &AppHandle) -> &'static str {
    app.try_state::<crate::AppState>().map(|s| s.core.settings().lang()).unwrap_or("fr")
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
        assert_eq!(pause_label(&st, "fr"), "Mettre l'écoute en pause");
        st.state = "paused".into();
        assert_eq!(pause_label(&st, "en"), "Resume listening");
    }

    #[test]
    fn fixed_labels_follow_settings_language() {
        assert_eq!(static_labels("fr").quit, "Quitter Sténo");
        assert_eq!(static_labels("en").quit, "Quit Sténo");
        assert_eq!(static_labels("en").settings, "Settings…");
    }

    #[test]
    fn panel_on_secondary_monitor() {
        // Main monitor 2880 px wide (Retina, ×2), 720 px panel.
        assert_eq!(panel_position(0.0, 0.0, 2880.0, 720.0, 2.0), (2136.0, 60.0));
        // Secondary monitor on the right (origin x = 2880), shifted upwards.
        assert_eq!(panel_position(2880.0, -200.0, 1920.0, 360.0, 1.0), (4428.0, -170.0));
    }
}
