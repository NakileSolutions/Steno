//! Permissions macOS demandées par Sténo : micro, audio du système, accessibilité.
//!
//! L'état est remonté par le sidecar (`permissions`) et gardé dans `runtime.permissions`.
//! Le sidecar sait aussi déclencher l'invite système (`request_permission`). Une fois un
//! droit refusé, macOS ne redemande plus : il faut ouvrir la bonne page des Réglages Système.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const KINDS: [&str; 4] = ["microphone", "system_audio", "accessibility", "calendar"];

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
}

impl Default for Permissions {
    fn default() -> Self {
        Self { microphone: unknown(), system_audio: unknown(), accessibility: unknown(), calendar: unknown(), available: false }
    }
}

pub fn check_kind(kind: &str) -> Result<()> {
    if !KINDS.contains(&kind) {
        bail!("permission inconnue : {kind}");
    }
    Ok(())
}

/// Page des Réglages Système correspondant au droit (adresses fixes, jamais fournies par l'UI).
pub fn settings_url(kind: &str) -> Result<&'static str> {
    Ok(match kind {
        "microphone" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        "system_audio" => "x-apple.systempreferences:com.apple.preference.security?Privacy_AudioCapture",
        "accessibility" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "calendar" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars",
        other => bail!("permission inconnue : {other}"),
    })
}

/// Ouvre la page des Réglages Système du droit demandé.
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
    #[cfg(not(target_os = "macos"))]
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
    }
}
