//! Permissions demandées par Sténo et informations système utiles à l'interface.
//!
//! macOS : micro, audio du système, accessibilité, calendriers. L'état est remonté par le
//! sidecar (`permissions`) et gardé dans `runtime.permissions`. Le sidecar sait aussi déclencher
//! l'invite système (`request_permission`). Une fois un droit refusé, macOS ne redemande plus :
//! il faut ouvrir la bonne page des Réglages Système.
//!
//! Linux : il n'y a pas d'autorisation à accorder (pas de TCC). Le sidecar remonte l'état de la
//! capture (micro, audio des apps) par le même message ; le cœur y ajoute le serveur son
//! (PulseAudio, ou PipeWire avec pipewire-pulse), la session graphique (Wayland ou X11) et l'état
//! des raccourcis globaux, enregistré par l'app (`runtime.app_shortcuts`).

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const KINDS: [&str; 4] = ["microphone", "system_audio", "accessibility", "calendar"];

/// Clé `runtime` où l'app note l'état de ses raccourcis globaux et sa ligne de commande.
pub const APP_SHORTCUTS_KEY: &str = "app_shortcuts";

fn unknown() -> String {
    "unknown".into()
}

/// État de chaque droit : `granted | denied | unknown`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Permissions {
    #[serde(default = "unknown")]
    pub microphone: String,
    #[serde(default = "unknown")]
    pub system_audio: String,
    #[serde(default = "unknown")]
    pub accessibility: String,
    /// Calendriers (facultatif) : réunions à venir et invités.
    #[serde(default = "unknown")]
    pub calendar: String,
    /// Le sidecar tourne : l'état est à jour et les demandes peuvent aboutir.
    #[serde(default)]
    pub available: bool,
    /// Système du cœur : `macos | linux | other`.
    #[serde(default)]
    pub platform: String,
    /// Linux : serveur son joignable (« PulseAudio (on PipeWire 1.0.5) ») ; `None` s'il est
    /// introuvable, ou hors Linux.
    #[serde(default)]
    pub sound_server: Option<String>,
    /// Session graphique (`wayland`, `x11`), `None` si inconnue.
    #[serde(default)]
    pub session: Option<String>,
    /// Les raccourcis globaux de l'app fonctionnent : `false` sous Wayland ou si leur
    /// enregistrement a échoué, `None` si l'app ne l'a pas encore indiqué.
    #[serde(default)]
    pub global_shortcuts: Option<bool>,
    /// Ligne de commande de l'app, à compléter par `--toggle`, `--aside`, `--note` ou `--show`
    /// pour un raccourci personnalisé du bureau (`None` si l'app ne l'a pas encore indiquée).
    #[serde(default)]
    pub app_command: Option<String>,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            microphone: unknown(),
            system_audio: unknown(),
            accessibility: unknown(),
            calendar: unknown(),
            available: false,
            platform: platform().into(),
            sound_server: None,
            session: None,
            global_shortcuts: None,
            app_command: None,
        }
    }
}

/// Raccourcis globaux de l'app, tels qu'elle les a notés au démarrage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AppShortcuts {
    /// Les raccourcis globaux fonctionnent (enregistrés, et hors Wayland).
    pub global: bool,
    /// Ligne de commande de l'app (chemin de l'exécutable, entre guillemets si besoin).
    pub command: String,
}

/// Système d'exploitation du cœur : `macos`, `linux` ou `other`.
pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    }
}

/// Session graphique d'après l'environnement : `wayland`, `x11` ou `None`.
pub fn session_type() -> Option<String> {
    session_from(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
    )
}

fn session_from(xdg: Option<&str>, wayland_display: bool, display: bool) -> Option<String> {
    match xdg.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("wayland") => return Some("wayland".into()),
        Some("x11") => return Some("x11".into()),
        _ => {}
    }
    if wayland_display {
        Some("wayland".into())
    } else if display {
        Some("x11".into())
    } else {
        None
    }
}

/// Nom du serveur son dans la sortie de `LC_ALL=C pactl info`.
pub fn parse_pactl_info(out: &str) -> Option<String> {
    let field = |key: &str| {
        out.lines()
            .find_map(|l| l.trim().strip_prefix(key))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let name = field("Server Name:")?;
    Some(match field("Server Version:") {
        // PipeWire : « PulseAudio (on PipeWire 1.0.5) » porte déjà sa version.
        Some(v) if !name.chars().any(|c| c.is_ascii_digit()) => format!("{name} {v}"),
        _ => name,
    })
}

/// Serveur son joignable par le protocole PulseAudio (PulseAudio, ou PipeWire avec
/// pipewire-pulse), `None` sinon. Linux seulement ; le résultat est gardé quelques secondes
/// (l'interface relit l'état régulièrement).
pub fn sound_server() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static CACHE: Mutex<Option<(Instant, Option<String>)>> = Mutex::new(None);
    if let Some((at, v)) = CACHE.lock().unwrap().as_ref() {
        if at.elapsed() < Duration::from_secs(5) {
            return v.clone();
        }
    }
    let v = probe_sound_server();
    *CACHE.lock().unwrap() = Some((Instant::now(), v.clone()));
    v
}

fn probe_sound_server() -> Option<String> {
    if let Ok(pactl) = which::which("pactl") {
        let out = std::process::Command::new(pactl)
            .arg("info")
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        return out.status.success().then(|| parse_pactl_info(&String::from_utf8_lossy(&out.stdout))).flatten();
    }
    // Sans pactl : la prise du protocole PulseAudio suffit à dire qu'un serveur écoute.
    pulse_socket().filter(|p| p.exists()).map(|_| "PulseAudio".to_string())
}

fn runtime_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()).map(std::path::PathBuf::from)
}

/// Prise du protocole PulseAudio (`$XDG_RUNTIME_DIR/pulse/native`).
pub fn pulse_socket() -> Option<std::path::PathBuf> {
    runtime_dir().map(|d| d.join("pulse").join("native"))
}

/// PipeWire tourne (`$XDG_RUNTIME_DIR/pipewire-0`) : utile pour conseiller pipewire-pulse.
pub fn pipewire_running() -> bool {
    runtime_dir().is_some_and(|d| d.join("pipewire-0").exists())
}

/// Raccourcis globaux notés par l'app.
pub fn app_shortcuts(db: &crate::Db) -> Option<AppShortcuts> {
    db.get_runtime(APP_SHORTCUTS_KEY).ok().flatten().and_then(|(v, _)| serde_json::from_str(&v).ok())
}

/// L'app note l'état de ses raccourcis globaux et sa ligne de commande (lus par `doctor` et
/// l'interface, y compris depuis la CLI).
pub fn record_app_shortcuts(db: &crate::Db, s: &AppShortcuts) -> Result<()> {
    db.set_runtime(APP_SHORTCUTS_KEY, &serde_json::to_string(s)?)
}

/// Complète l'état avec ce que le cœur sait du système.
pub fn with_system_info(mut p: Permissions, db: &crate::Db) -> Permissions {
    p.platform = platform().into();
    p.session = session_type();
    p.sound_server = sound_server();
    if let Some(s) = app_shortcuts(db) {
        p.global_shortcuts = Some(s.global);
        p.app_command = Some(s.command).filter(|c| !c.is_empty());
    }
    p
}

pub fn check_kind(kind: &str) -> Result<()> {
    if !KINDS.contains(&kind) {
        bail!("permission inconnue : {kind}");
    }
    Ok(())
}

/// Page des Réglages Système (macOS) correspondant au droit (adresses fixes, jamais fournies
/// par l'UI).
pub fn settings_url(kind: &str) -> Result<&'static str> {
    Ok(match kind {
        "microphone" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        "system_audio" => "x-apple.systempreferences:com.apple.preference.security?Privacy_AudioCapture",
        "accessibility" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "calendar" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars",
        other => bail!("permission inconnue : {other}"),
    })
}

/// Linux : réglages du son à ouvrir pour le micro et l'audio des apps (programme, arguments),
/// dans l'ordre de préférence.
pub const LINUX_SOUND_SETTINGS: &[(&str, &[&str])] = &[("gnome-control-center", &["sound"]), ("pavucontrol", &[])];

/// Ouvre la page des réglages du droit demandé : Réglages Système sur macOS, réglages du son
/// sous Linux (micro et audio des apps seulement).
pub fn open_settings(kind: &str) -> Result<()> {
    let url = settings_url(kind)?;
    #[cfg(target_os = "macos")]
    {
        let st = std::process::Command::new("/usr/bin/open").arg(url).status()?;
        if !st.success() {
            bail!("impossible d'ouvrir les Réglages Système");
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let _ = url;
        if !matches!(kind, "microphone" | "system_audio") {
            bail!("aucun réglage à ouvrir pour ce droit sous Linux");
        }
        for (prog, args) in LINUX_SOUND_SETTINGS {
            let Ok(path) = which::which(prog) else { continue };
            let mut child = std::process::Command::new(path)
                .args(*args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            // Attendu dans un fil à part : pas de processus zombie, et l'appel rend la main.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(());
        }
        bail!("réglages du son introuvables : installez gnome-control-center ou pavucontrol")
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = url;
        bail!("Réglages Système : macOS uniquement")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn droits_connus_et_adresses_fixes() {
        for k in KINDS {
            assert!(check_kind(k).is_ok());
            assert!(settings_url(k).unwrap().starts_with("x-apple.systempreferences:"));
        }
        assert!(check_kind("camera").is_err());
        assert!(settings_url("https://exemple.fr").is_err());
    }

    #[test]
    fn etat_par_defaut_inconnu() {
        let p: Permissions = serde_json::from_str(r#"{"microphone":"granted"}"#).unwrap();
        assert_eq!(p.microphone, "granted");
        assert_eq!(p.system_audio, "unknown");
        assert!(!p.available);
        assert_eq!(p.sound_server, None);
        assert_eq!(Permissions::default().platform, platform());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_rien_a_ouvrir_hors_du_son() {
        let e = open_settings("accessibility").unwrap_err().to_string();
        assert!(e.contains("sous Linux"), "{e}");
        assert!(open_settings("calendar").is_err());
        assert!(open_settings("camera").unwrap_err().to_string().contains("inconnue"));
    }

    #[test]
    fn session_graphique() {
        assert_eq!(session_from(Some("wayland"), false, true).as_deref(), Some("wayland"));
        assert_eq!(session_from(Some("x11"), true, true).as_deref(), Some("x11"));
        assert_eq!(session_from(Some("tty"), true, false).as_deref(), Some("wayland"));
        assert_eq!(session_from(None, false, true).as_deref(), Some("x11"));
        assert_eq!(session_from(None, false, false), None);
    }

    #[test]
    fn serveur_son_de_pactl() {
        let pw = "Server String: /run/user/1000/pulse/native\nServer Protocol Version: 35\nServer Name: PulseAudio (on PipeWire 1.0.5)\nServer Version: 15.0.0\n";
        assert_eq!(parse_pactl_info(pw).as_deref(), Some("PulseAudio (on PipeWire 1.0.5)"));
        let pa = "Server Name: pulseaudio\nServer Version: 16.1\n";
        assert_eq!(parse_pactl_info(pa).as_deref(), Some("pulseaudio 16.1"));
        assert_eq!(parse_pactl_info("Connection failure: Connection refused\n"), None);
    }

    #[test]
    fn raccourcis_notes_par_l_app() {
        let db = crate::Db::open_in_memory().unwrap();
        assert!(app_shortcuts(&db).is_none());
        let p = with_system_info(Permissions::default(), &db);
        assert_eq!(p.global_shortcuts, None);
        record_app_shortcuts(&db, &AppShortcuts { global: false, command: "/usr/bin/steno-app".into() }).unwrap();
        let p = with_system_info(Permissions::default(), &db);
        assert_eq!(p.global_shortcuts, Some(false));
        assert_eq!(p.app_command.as_deref(), Some("/usr/bin/steno-app"));
    }
}
