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

fn macos_version() -> Option<(u32, u32, u32)> {
    let out = std::process::Command::new("sw_vers").arg("-productVersion").output().ok()?;
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let mut p = v.split('.').map(|x| x.parse::<u32>().unwrap_or(0));
    Some((p.next()?, p.next().unwrap_or(0), p.next().unwrap_or(0)))
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
    if cfg!(target_os = "macos") {
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
        let audio = core.settings().audio;
        let (mut found, mut missing) = (vec![], vec![]);
        for app in audio.call_apps.iter().filter(|a| a.enabled) {
            let installed = app.bundles.iter().any(|b| {
                std::process::Command::new("mdfind")
                    .arg(format!("kMDItemCFBundleIdentifier == '{b}'"))
                    .output()
                    .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
                    .unwrap_or(false)
            });
            if installed {
                found.push(app.name.clone());
            } else {
                missing.push(app.name.clone());
            }
        }
        v.push(if found.is_empty() {
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
        });
    } else {
        v.push(item("Système", "warn", "hors macOS : capture audio indisponible (mode simulation possible)", None));
    }

    // Base
    match inner.db.path() {
        Some(p) => v.push(item("Base de données", "ok", p.display().to_string(), None)),
        None => v.push(item("Base de données", "warn", "base en mémoire", None)),
    }

    // Sidecar et permissions
    let live = core.live_state();
    v.push(match live.sidecar.as_str() {
        "running" => item("Capture audio", "ok", "sidecar steno-audio lancé", None),
        "missing" => item(
            "Capture audio",
            "error",
            "sidecar steno-audio introuvable",
            Some("Recompiler avec `make install` (il est embarqué dans Steno.app)."),
        ),
        "starting" => item("Capture audio", "warn", "sidecar en cours de démarrage", None),
        other => item(
            "Capture audio",
            "error",
            format!("sidecar : {other} {}", live.last_error.clone().unwrap_or_default()),
            Some("Voir les journaux dans ~/Library/Application Support/Steno/logs."),
        ),
    });
    if let Ok(Some((perms, _))) = inner.db.get_runtime("permissions") {
        if let Ok(p) = serde_json::from_str::<serde_json::Value>(&perms) {
            for (key, label, fix) in [
                ("microphone", "Micro", "Réglages Système → Confidentialité et sécurité → Micro → cocher Sténo."),
                ("system_audio", "Audio du système", "Réglages Système → Confidentialité et sécurité → Enregistrement de l'écran et de l'audio système → cocher Sténo. Si la case est absente ou grisée, le MDM de l'entreprise la bloque (spike S4)."),
                ("accessibility", "Accessibilité", "Réglages Système → Confidentialité et sécurité → Accessibilité → cocher Sténo (titre de la réunion Teams)."),
            ] {
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
        Err(e) => item("Politique lecture seule", "error", e, Some("Vérifier ~/Library/Application Support/Steno/policy.toml.")),
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
    #[test]
    fn auto_test_politique() {
        super::policy_self_test().unwrap();
    }
}
