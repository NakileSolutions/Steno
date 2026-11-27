//! Diagnostic (`steno doctor`, écran d'accueil) : ✅ / ⚠️ / ❌ avec la marche à suivre.

use crate::agents::{SessionMode, ToolAccess};
use crate::core::Core;
use crate::model::Tier;
use crate::policy::{Decision, ReadOnlyPolicy};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DoctorItem {
    pub name: String,
    pub status: String,
    pub message: String,
    pub fix: Option<String>,
}

fn item(name: &str, status: &str, message: impl Into<String>, fix: Option<&str>) -> DoctorItem {
    DoctorItem { name: name.into(), status: status.into(), message: message.into(), fix: fix.map(String::from) }
}

#[cfg(target_os = "macos")]
fn macos_version() -> Option<(u32, u32, u32)> {
    let out = std::process::Command::new("sw_vers").arg("-productVersion").output().ok()?;
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let mut p = v.split('.').map(|x| x.parse::<u32>().unwrap_or(0));
    Some((p.next()?, p.next().unwrap_or(0), p.next().unwrap_or(0)))
}

/// Nom de la distribution dans `/etc/os-release` (`PRETTY_NAME`, sinon `NAME VERSION`).
pub fn parse_os_release(text: &str) -> Option<String> {
    let field = |key: &str| {
        text.lines().find_map(|l| {
            let v = l.trim().strip_prefix(key)?.strip_prefix('=')?;
            let v = v.trim().trim_matches('"').trim_matches('\'').trim();
            (!v.is_empty()).then(|| v.to_string())
        })
    };
    field("PRETTY_NAME").or_else(|| {
        let name = field("NAME")?;
        Some(match field("VERSION") {
            Some(v) => format!("{name} {v}"),
            None => name,
        })
    })
}

/// Identifiants des apps Flatpak installées (vide sans flatpak).
#[cfg(target_os = "linux")]
fn flatpak_apps() -> Vec<String> {
    let Ok(flatpak) = which::which("flatpak") else { return vec![] };
    std::process::Command::new(flatpak)
        .args(["list", "--app", "--columns=application"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

/// Une app de visio est installée sous Linux : un de ses `bundles` est un exécutable du PATH
/// (Snap compris, via `/snap/bin`) ou le préfixe (sans casse) d'une app Flatpak installée.
pub fn linux_app_installed(bundles: &[String], flatpaks: &[String], in_path: impl Fn(&str) -> bool) -> bool {
    bundles.iter().any(|b| {
        let lower = b.to_lowercase();
        (!b.contains('/') && in_path(b)) || flatpaks.iter().any(|f| f.to_lowercase().starts_with(&lower))
    })
}

/// Le dossier des modèles contient Parakeet (sous-dossier dont le nom contient « parakeet »).
pub fn parakeet_present(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().any(|e| e.path().is_dir() && e.file_name().to_string_lossy().to_lowercase().contains("parakeet"))
        })
        .unwrap_or(false)
}

/// Commande à lier à un raccourci personnalisé du bureau (Wayland) : `<app> --toggle`…
pub fn app_command(db: &crate::Db) -> String {
    crate::permissions::app_shortcuts(db).map(|s| s.command).filter(|c| !c.is_empty()).unwrap_or_else(|| "steno-app".into())
}

/// Apps de visio surveillées, réparties en installées / absentes selon `installed`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn call_apps_item(core: &Core, installed: impl Fn(&crate::settings::CallApp) -> bool) -> DoctorItem {
    let audio = core.settings().audio;
    let (mut found, mut missing) = (vec![], vec![]);
    for app in audio.call_apps.iter().filter(|a| a.enabled) {
        if installed(app) {
            found.push(app.name.clone());
        } else {
            missing.push(app.name.clone());
        }
    }
    if found.is_empty() {
        item(
            "Apps de visio",
            "warn",
            "aucune app de visio surveillée n'est installée",
            Some("Réglages → Audio & transcription → Apps de visio."),
        )
    } else {
        let mut msg = format!("surveillées et installées : {}", found.join(", "));
        if !missing.is_empty() {
            msg.push_str(&format!(" (absentes : {})", missing.join(", ")));
        }
        item("Apps de visio", "ok", msg, None)
    }
}

/// Vérifications propres à Linux : distribution, serveur son, session graphique, apps de
/// visio, modèles audio.
#[cfg(target_os = "linux")]
fn linux_checks(core: &Core, v: &mut Vec<DoctorItem>) {
    let distro = std::fs::read_to_string("/etc/os-release").ok().and_then(|t| parse_os_release(&t));
    v.push(match distro {
        Some(d) => item("Système", "ok", d, None),
        None => item("Système", "warn", "distribution inconnue (/etc/os-release illisible)", None),
    });

    v.push(match crate::permissions::sound_server() {
        Some(name) => item("Serveur son", "ok", name, None),
        None if crate::permissions::pipewire_running() => item(
            "Serveur son",
            "error",
            "PipeWire tourne sans pipewire-pulse : le son des apps ne peut pas être capté",
            Some("Installer pipewire-pulse (`sudo apt install pipewire-pulse`), puis se reconnecter."),
        ),
        None => item(
            "Serveur son",
            "error",
            "aucun serveur son PulseAudio ou PipeWire joignable",
            Some("Installer PipeWire avec pipewire-pulse (`sudo apt install pipewire-pulse`) ou PulseAudio, puis se reconnecter."),
        ),
    });

    let cmd = app_command(&core.inner.db);
    let bind_fix = format!(
        "Paramètres → Clavier → Raccourcis personnalisés : « {cmd} --toggle » (démarrer/arrêter l'écoute), « {cmd} --aside » (aparté), « {cmd} --note » (note rapide)."
    );
    let shortcuts = crate::permissions::app_shortcuts(&core.inner.db);
    v.push(match crate::permissions::session_type().as_deref() {
        Some("wayland") => item("Session", "warn", "Wayland : les raccourcis globaux de Sténo sont indisponibles", Some(bind_fix.as_str())),
        Some(_) if shortcuts.as_ref().is_some_and(|s| !s.global) => {
            item("Session", "warn", "X11 : raccourcis globaux refusés (déjà pris par une autre app ?)", Some(bind_fix.as_str()))
        }
        Some(_) => item("Session", "ok", "X11 : raccourcis globaux disponibles", None),
        None => item("Session", "warn", "aucune session graphique détectée", None),
    });

    let flatpaks = flatpak_apps();
    let cwd = std::env::temp_dir();
    let path = std::env::var("PATH").unwrap_or_default();
    v.push(call_apps_item(core, |app| {
        linux_app_installed(&app.bundles, &flatpaks, |b| which::which_in(b, Some(&path), &cwd).is_ok())
    }));

    let models = crate::paths::models_dir();
    v.push(if parakeet_present(&models) {
        item("Modèles audio", "ok", models.display().to_string(), None)
    } else {
        item(
            "Modèles audio",
            "error",
            format!("modèle de transcription Parakeet absent de {}", models.display()),
            Some("Lancer `make install` (téléchargement des modèles audio), ou indiquer leur dossier avec STENO_MODELS_DIR."),
        )
    });
}

/// Écrit sur des outils d'écriture connus ? (vérifie que la politique les refuse).
pub fn policy_self_test() -> Result<(), String> {
    let mut p = ReadOnlyPolicy::no_tools();
    p.tools_enabled = true;
    p.allowed_mcp_servers = vec!["slack".into(), "atlassian".into(), "datadog".into(), "gitlab".into()];
    let cwd = std::env::temp_dir();
    for t in [
        "mcp__slack__slack_send_message",
        "mcp__atlassian__createJiraIssue",
        "mcp__atlassian__addCommentToJiraIssue",
        "mcp__atlassian__transitionJiraIssue",
        "mcp__atlassian__updateConfluencePage",
        "mcp__gitlab__merge_merge_request",
        "mcp__datadog__mute_monitor",
    ] {
        if p.decide(t, &serde_json::json!({}), &cwd) == Decision::Allow {
            return Err(format!("la politique laisse passer {t}"));
        }
    }
    for c in ["git push", "rm -rf .", "curl https://example.com", "rg x > f"] {
        if p.check_command(c, &cwd).is_ok() {
            return Err(format!("la politique laisse passer « {c} »"));
        }
    }
    Ok(())
}

/// Diagnostic complet. `live_policy_test` : interroge réellement l'agent (consomme un appel).
pub async fn run(core: &Core, live_policy_test: bool) -> Vec<DoctorItem> {
    let mut v = vec![];
    let inner = &core.inner;

    // Système
    #[cfg(target_os = "macos")]
    {
        match macos_version() {
            Some((maj, min, patch)) if (maj, min) >= (14, 2) => {
                v.push(item("macOS", "ok", format!("macOS {maj}.{min}.{patch}"), None))
            }
            Some((maj, min, patch)) => v.push(item(
                "macOS",
                "error",
                format!("macOS {maj}.{min}.{patch} : les process taps Core Audio demandent macOS 14.2+"),
                Some("Mettre à jour macOS."),
            )),
            None => v.push(item("macOS", "warn", "version inconnue", None)),
        }
        // Apps de visio surveillées : lesquelles sont installées (recherche Spotlight par bundle).
        v.push(call_apps_item(core, |app| {
            app.bundles.iter().any(|b| {
                std::process::Command::new("mdfind")
                    .arg(format!("kMDItemCFBundleIdentifier == '{b}'"))
                    .output()
                    .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
                    .unwrap_or(false)
            })
        }));
    }
    #[cfg(target_os = "linux")]
    linux_checks(core, &mut v);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    v.push(item("Système", "warn", "système non pris en charge : capture audio indisponible (mode simulation possible)", None));

    // Base
    match inner.db.path() {
        Some(p) => v.push(item("Base de données", "ok", p.display().to_string(), None)),
        None => v.push(item("Base de données", "warn", "base en mémoire", None)),
    }

    // Sidecar et permissions
    let logs_fix = format!("Voir les journaux dans {}.", crate::paths::logs_dir().display());
    let live = core.live_state();
    v.push(match live.sidecar.as_str() {
        "running" => item("Capture audio", "ok", "sidecar steno-audio lancé", None),
        "missing" => item(
            "Capture audio",
            "error",
            "sidecar steno-audio introuvable",
            Some(if cfg!(target_os = "macos") {
                "Recompiler avec `make install` (il est embarqué dans Steno.app)."
            } else {
                "Réinstaller avec `make install` : steno-audio doit se trouver à côté de steno-app."
            }),
        ),
        "starting" => item("Capture audio", "warn", "sidecar en cours de démarrage", None),
        other => item(
            "Capture audio",
            "error",
            format!("sidecar : {other} {}", live.last_error.clone().unwrap_or_default()),
            Some(logs_fix.as_str()),
        ),
    });
    if let Ok(Some((perms, _))) = inner.db.get_runtime("permissions") {
        if let Ok(p) = serde_json::from_str::<serde_json::Value>(&perms) {
            let rows: &[(&str, &str, &str)] = if cfg!(target_os = "macos") {
                &[
                    ("microphone", "Micro", "Réglages Système → Confidentialité et sécurité → Micro → cocher Sténo."),
                    ("system_audio", "Audio du système", "Réglages Système → Confidentialité et sécurité → Enregistrement de l'écran et de l'audio système → cocher Sténo. Si la case est absente ou grisée, le MDM de l'entreprise la bloque (spike S4)."),
                    ("accessibility", "Accessibilité", "Réglages Système → Confidentialité et sécurité → Accessibilité → cocher Sténo (titre de la réunion Teams)."),
                ]
            } else {
                // Linux : rien à autoriser ; un refus vient du serveur son ou d'un confinement.
                &[
                    ("microphone", "Micro", "Vérifier le micro choisi dans les réglages du son (gnome-control-center sound ou pavucontrol), et que Sténo n'est pas confiné (Flatpak, Snap)."),
                    ("system_audio", "Audio du système", "Vérifier le serveur son (`pactl info`) et que Sténo n'est pas confiné (Flatpak, Snap)."),
                ]
            };
            for &(key, label, fix) in rows {
                let st = p.get(key).and_then(|x| x.as_str()).unwrap_or("unknown");
                v.push(match st {
                    "granted" => item(label, "ok", "autorisé", None),
                    "denied" => item(label, if key == "accessibility" { "warn" } else { "error" }, "refusé", Some(fix)),
                    _ => item(label, "warn", "état inconnu (vérifié au premier appel)", Some(fix)),
                });
            }
        }
    }

    // Agent
    let s = core.settings();
    match inner.adapter() {
        Ok(a) => {
            let d = a.detect().await;
            if d.installed {
                v.push(item(
                    "Agent IA",
                    "ok",
                    format!("{} — {}", a.display_name(), d.version.unwrap_or_else(|| "version inconnue".into())),
                    None,
                ));
            } else {
                v.push(item(
                    "Agent IA",
                    "error",
                    format!("{} : {}", a.display_name(), d.error.unwrap_or_default()),
                    Some("Installer le CLI de l'agent et s'y connecter, ou en choisir un autre dans Réglages → Agent IA."),
                ));
            }
            let caps = a.capabilities();
            if !caps.tool_deny_list {
                v.push(item(
                    "Connecteurs",
                    "warn",
                    format!("{} ne sait pas bloquer les outils MCP en écriture : connecteurs désactivés", a.display_name()),
                    None,
                ));
            } else {
                let cons = inner.db.list_connectors(Some(a.id())).unwrap_or_default();
                let active = cons.iter().filter(|c| c.use_live || c.use_correlation).count();
                v.push(item(
                    "Connecteurs",
                    if cons.is_empty() { "warn" } else { "ok" },
                    format!("{} détecté(s), {} activé(s)", cons.len(), active),
                    cons.is_empty().then_some("Réglages → Connecteurs → Rafraîchir."),
                ));
            }
        }
        Err(e) => v.push(item("Agent IA", "error", e.to_string(), None)),
    }
    for k in ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY"] {
        if std::env::var(k).map(|x| !x.is_empty()).unwrap_or(false) {
            v.push(item(
                "Clé d'API",
                if s.agent.allow_api_key { "warn" } else { "ok" },
                if s.agent.allow_api_key {
                    format!("{k} présente et autorisée : appels facturés hors abonnement")
                } else {
                    format!("{k} présente mais ignorée (non transmise à l'agent)")
                },
                None,
            ));
        }
    }

    // Politique lecture seule
    v.push(match policy_self_test() {
        Ok(()) => item("Politique lecture seule", "ok", "les outils d'écriture et commandes dangereuses sont refusés", None),
        Err(e) => item(
            "Politique lecture seule",
            "error",
            e,
            Some(format!("Vérifier {}.", crate::paths::home().join("policy.toml").display()).as_str()),
        ),
    });
    if live_policy_test {
        let connectors = inner.sources(true).map(|s| s.connectors).unwrap_or_default();
        let res = inner
            .run_agent(None, Tier::Quick, ToolAccess::ReadOnly { connectors }, SessionMode::None, crate::prompts::policy_test(), 5, None)
            .await;
        v.push(match res {
            Ok(r) => {
                let writes: Vec<&String> = r
                    .tools_used
                    .iter()
                    .filter(|t| {
                        let tool = t.rsplit("__").next().unwrap_or(t);
                        ReadOnlyPolicy::no_tools().mcp_tool_denied(tool).is_some()
                    })
                    .collect();
                if writes.is_empty() {
                    item("Test réel « poste sur Slack »", "ok", "refusé par l'agent, aucun outil d'écriture exécuté", None)
                } else {
                    item(
                        "Test réel « poste sur Slack »",
                        "error",
                        format!("outils d'écriture exécutés : {writes:?}"),
                        Some("Désactiver tous les connecteurs et signaler le problème."),
                    )
                }
            }
            Err(e) => item("Test réel « poste sur Slack »", "warn", format!("test impossible : {e}"), None),
        });
    }
    let lang = s.lang();
    let tr = |t: &str| crate::i18n_messages::localize(lang, t);
    v.into_iter()
        .map(|i| DoctorItem { name: tr(&i.name), message: tr(&i.message), fix: i.fix.as_deref().map(tr), ..i })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_test_politique() {
        super::policy_self_test().unwrap();
    }

    #[test]
    fn distribution_linux() {
        let ubuntu = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nID=ubuntu\n";
        assert_eq!(parse_os_release(ubuntu).as_deref(), Some("Ubuntu 24.04.1 LTS"));
        assert_eq!(parse_os_release("NAME='Debian GNU/Linux'\nVERSION=\"12 (bookworm)\"\n").as_deref(), Some("Debian GNU/Linux 12 (bookworm)"));
        assert_eq!(parse_os_release("ID=x\n"), None);
    }

    #[test]
    fn apps_installees_sous_linux() {
        let flatpaks = vec!["com.slack.Slack".to_string(), "us.zoom.Zoom".to_string()];
        let path = |b: &str| b == "teams-for-linux";
        let b = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(linux_app_installed(&b(&["teams-for-linux", "teams"]), &[], path));
        assert!(linux_app_installed(&b(&["slack", "com.slack.slack"]), &flatpaks, path), "préfixe sans casse");
        assert!(linux_app_installed(&b(&["zoom", "us.zoom"]), &flatpaks, path));
        assert!(!linux_app_installed(&b(&["discord", "com.discordapp.Discord"]), &flatpaks, path));
    }

    #[test]
    fn modeles_audio() {
        let d = tempfile::tempdir().unwrap();
        assert!(!parakeet_present(d.path()));
        std::fs::write(d.path().join("parakeet.txt"), b"").unwrap();
        assert!(!parakeet_present(d.path()), "un fichier ne compte pas");
        std::fs::create_dir(d.path().join("Parakeet-TDT-0.6b-v3")).unwrap();
        assert!(parakeet_present(d.path()));
        assert!(!parakeet_present(&d.path().join("absent")));
    }
}
