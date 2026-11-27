//! Répartiteur générique de l'API de l'app (docs/contracts.md §5) : nom de commande + arguments
//! JSON (camelCase, comme Tauri) → résultat JSON. Utilisé par `steno serve` (backend HTTP des
//! tests e2e et du développement sans Tauri). L'app Tauri expose les mêmes commandes, typées.

use crate::core::Core;
use crate::model::{ActionFilter, MeetingFilter, Repo};
use crate::settings::Settings;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

/// Commandes du contrat (l'ordre suit docs/contracts.md §5).
pub const COMMANDS: &[&str] = &[
    "live_state", "start_meeting", "stop_meeting", "set_aside", "set_live_confidential", "ask", "ask_history",
    "cancel_request", "recent_requests", "list_meetings", "get_meeting", "get_transcript", "get_report_version",
    "rename_meeting", "set_confidential", "delete_meeting", "rename_speaker", "add_note", "update_note",
    "delete_note", "regenerate_report", "correlate", "export_meeting", "search", "list_actions",
    "set_action_status", "list_decisions", "get_settings", "save_settings", "export_settings", "import_settings",
    "detect_agents", "list_audio_devices", "list_models", "test_agent", "list_connectors", "refresh_connectors", "update_connector", "list_repos",
    "add_repo", "update_repo", "remove_repo", "pick_folder", "check_wake_word", "generate_variants",
    "check_variants", "wake_detect", "wake_test_record", "doctor", "delete_all_data", "log_ui_error",
    "get_permissions", "request_permission", "open_permission_settings", "complete_onboarding",
    "delete_segment", "set_mic_muted", "save_export", "print_window", "pause_meeting", "resume_meeting",
    "get_meeting_context", "set_meeting_context", "add_context_file", "delete_context_file", "list_report_templates",
    "report_now", "get_update_status", "check_update", "install_update", "restart_app",
    "upcoming", "plan_meeting", "start_planned", "get_participants", "set_participants", "prepare_questions", "prepare_apply",
];

struct Args(Value);

impl Args {
    fn get<T: DeserializeOwned>(&self, k: &str) -> Result<T, String> {
        let v = self.0.get(k).cloned().unwrap_or(Value::Null);
        serde_json::from_value(v).map_err(|e| format!("argument « {k} » invalide : {e}"))
    }
    fn opt<T: DeserializeOwned>(&self, k: &str) -> Result<Option<T>, String> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => serde_json::from_value(v.clone()).map(Some).map_err(|e| format!("argument « {k} » invalide : {e}")),
        }
    }
    fn s(&self, k: &str) -> Result<String, String> {
        self.get::<String>(k)
    }
}

fn ok<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn e(err: anyhow::Error) -> String {
    format!("{err:#}")
}

/// Exécute une commande. Erreur = message dans la langue des réglages.
pub async fn dispatch(core: &Core, cmd: &str, args: Value) -> Result<Value, String> {
    run(core, cmd, args).await.map_err(|m| crate::i18n_messages::localize(core.settings().lang(), &m))
}

async fn run(core: &Core, cmd: &str, args: Value) -> Result<Value, String> {
    let a = Args(if args.is_null() { json!({}) } else { args });
    let db = core.db();
    match cmd {
        "live_state" => ok(core.live_state()),
        "start_meeting" => ok(core
            .start_meeting_with(a.opt("title")?, a.opt("in_person")?.unwrap_or(false))
            .await
            .map_err(e)?),
        "stop_meeting" => ok(core.stop_meeting().await.map_err(e)?),
        "set_aside" => {
            core.set_aside(a.get("on")?);
            ok(())
        }
        "set_live_confidential" => ok(core.set_live_confidential(a.get("on")?).map_err(e)?),
        "ask" => ok(core.ask(&a.s("query")?, a.opt::<String>("meetingId")?.as_deref(), "text").map_err(e)?),
        "ask_history" => ok(core.ask_history(&a.s("query")?, "text").map_err(e)?),
        "cancel_request" => ok(core.cancel_request(&a.s("id")?).map_err(e)?),
        "recent_requests" => ok(core.recent_requests(a.opt::<i64>("limit")?.unwrap_or(5).clamp(1, 200)).map_err(e)?),
        "list_meetings" => ok(db.list_meetings(&a.opt::<MeetingFilter>("filter")?.unwrap_or_default()).map_err(e)?),
        "get_meeting" => ok(db.meeting_detail(&a.s("id")?).map_err(e)?.ok_or("réunion introuvable")?),
        "get_transcript" => ok(db.segments(&a.s("id")?, a.opt("includePrivate")?.unwrap_or(false), true).map_err(e)?),
        "get_report_version" => {
            ok(db.get_report_version(&a.s("id")?, a.get("version")?).map_err(e)?.ok_or("version introuvable")?)
        }
        "rename_meeting" => ok(core.rename_meeting(&a.s("id")?, &a.s("title")?).map_err(e)?),
        "set_confidential" => ok(core.set_confidential(&a.s("id")?, a.get("on")?).map_err(e)?),
        "delete_meeting" => ok(core.delete_meeting(&a.s("id")?).map_err(e)?),
        "rename_speaker" => ok(db.rename_speaker(&a.s("meetingId")?, &a.s("label")?, &a.s("displayName")?).map_err(e)?),
        "add_note" => ok(core.add_note(&a.s("meetingId")?, &a.s("text")?).map_err(e)?),
        "update_note" => ok(db.update_note(a.get("id")?, &a.s("text")?).map_err(e)?),
        "delete_note" => ok(db.delete_note(a.get("id")?).map_err(e)?),
        "regenerate_report" => ok(core.regenerate_report(&a.s("id")?).map_err(e)?),
        "get_meeting_context" => ok(core.meeting_context(&a.s("id")?).map_err(e)?),
        "set_meeting_context" => {
            let template: Option<String> = a.opt("template")?;
            ok(core.set_meeting_context(&a.s("id")?, &a.s("text")?, template.as_deref()).map_err(e)?)
        }
        "add_context_file" => ok(core.add_context_file(&a.s("id")?, &a.s("name")?, &a.s("data")?).map_err(e)?),
        "delete_context_file" => ok(core.delete_context_file(a.get("fileId")?).map_err(e)?),
        "list_report_templates" => ok(core.report_templates()),
        "report_now" => ok(core.report_now(&a.s("id")?).map_err(e)?),
        "get_update_status" => ok(core.update_status()),
        "upcoming" => {
            let days: Option<i64> = a.opt("days")?;
            ok(core.upcoming(days.unwrap_or(7)).await.map_err(e)?)
        }
        "plan_meeting" => {
            let event_id: Option<String> = a.opt("eventId")?;
            ok(core.plan_meeting(&a.s("title")?, &a.s("scheduledAt")?, event_id.as_deref()).map_err(e)?)
        }
        "start_planned" => {
            let in_person: Option<bool> = a.opt("inPerson")?;
            ok(core.start_planned(&a.s("id")?, in_person.unwrap_or(false)).await.map_err(e)?)
        }
        "get_participants" => ok(core.participants(&a.s("id")?).map_err(e)?),
        "set_participants" => {
            let list: Vec<crate::planning::Participant> = a.get("participants")?;
            ok(core.set_participants(&a.s("id")?, &list).map_err(e)?)
        }
        "prepare_questions" => ok(core.prepare_questions(&a.s("id")?).await.map_err(e)?),
        "prepare_apply" => {
            let answers: Vec<crate::planning::PrepAnswer> = a.get("answers")?;
            ok(core.prepare_apply(&a.s("id")?, &answers).await.map_err(e)?)
        }
        "check_update" => ok(core.check_update().await.map_err(e)?),
        "install_update" => ok(core.install_update().map_err(e)?),
        // Redémarrage : propre à l'app Tauri.
        "restart_app" => Ok(Value::Null),
        "correlate" => ok(core.correlate(&a.s("id")?).map_err(e)?),
        "export_meeting" => {
            let format: Option<String> = a.opt("format")?;
            ok(core.export_meeting_as(&a.s("id")?, format.as_deref().unwrap_or("markdown")).map_err(e)?)
        }
        // Fenêtre d'enregistrement et impression : seulement dans l'app (l'interface web
        // télécharge le fichier ou appelle `window.print()` elle-même).
        "save_export" | "print_window" => ok(Value::Null),
        "search" => ok(db.search(&a.s("query")?, 100).map_err(e)?),
        "list_actions" => ok(db.list_actions(&a.opt::<ActionFilter>("filter")?.unwrap_or_default()).map_err(e)?),
        "set_action_status" => {
            let st = a.s("status")?;
            if st != "open" && st != "done" {
                return Err("statut inconnu".into());
            }
            ok(db.set_action_status(a.get("id")?, &st).map_err(e)?)
        }
        "list_decisions" => ok(db.list_decisions(a.opt::<String>("since")?.as_deref()).map_err(e)?),
        "get_settings" => ok(core.settings()),
        "save_settings" => ok(core.save_settings(a.get::<Settings>("settings")?).map_err(e)?),
        "export_settings" => ok(core.export_settings().map_err(e)?),
        "import_settings" => ok(core.import_settings(&a.s("toml")?).map_err(e)?),
        "detect_agents" => ok(core.detect_agents().await),
        "list_audio_devices" => ok(core.audio_devices()),
        "list_models" => ok(core.list_models(&a.s("provider")?).await.map_err(e)?),
        "test_agent" => {
            let (okk, message, ms) =
                core.test_agent_with(&a.s("provider")?, a.opt("model")?, a.opt("effort")?).await.map_err(e)?;
            ok(json!({ "ok": okk, "message": message, "duration_ms": ms }))
        }
        "list_connectors" => ok(core.list_connectors().map_err(e)?),
        "refresh_connectors" => ok(core.refresh_connectors().await.map_err(e)?),
        "update_connector" => {
            ok(db.update_connector(a.get("id")?, a.get("useLive")?, a.get("useCorrelation")?).map_err(e)?)
        }
        "list_repos" => ok(db.list_repos().map_err(e)?),
        "add_repo" => {
            let path = a.s("path")?;
            let p = std::path::Path::new(&path);
            if !p.is_dir() {
                return Err(format!("dossier introuvable : {path}"));
            }
            let abs = p.canonicalize().map_err(|x| x.to_string())?;
            let label = a
                .opt::<String>("label")?
                .filter(|l| !l.trim().is_empty())
                .or_else(|| abs.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_default();
            ok(db.add_repo(&abs.display().to_string(), &label).map_err(e)?)
        }
        "update_repo" => ok(db.update_repo(&a.get::<Repo>("repo")?).map_err(e)?),
        "remove_repo" => ok(db.remove_repo(a.get("id")?).map_err(e)?),
        // Pas de sélecteur de dossier hors de l'app : l'interface propose la saisie du chemin.
        "pick_folder" => ok(Value::Null),
        "check_wake_word" => ok(core.check_wake_word(&a.s("word")?, a.opt("strict")?.unwrap_or(false)).map_err(e)?),
        "generate_variants" => ok(core.generate_variants(&a.s("word")?)),
        "check_variants" => ok(core.check_variants(&a.s("word")?, &a.get::<Vec<String>>("variants")?)),
        "wake_detect" => ok(match core.wake_detect(&a.s("text")?) {
            Some(d) => json!({ "detected": true, "intent": d.intent.as_str(), "query": d.query }),
            None => json!({ "detected": false, "intent": null, "query": null }),
        }),
        "wake_test_record" => {
            let secs: u32 = a.opt("seconds")?.unwrap_or(5);
            let secs = secs.clamp(1, 60);
            let text = tokio::time::timeout(std::time::Duration::from_secs(secs as u64 + 30), core.wake_test_record(secs))
                .await
                .map_err(|_| "le micro n'a rien renvoyé à temps".to_string())?
                .map_err(e)?;
            ok(json!({ "text": text }))
        }
        "doctor" => ok(crate::doctor::run(core, false).await),
        "delete_all_data" => ok(core.delete_all_data().map_err(e)?),
        "get_permissions" => ok(core.permissions()),
        "request_permission" => ok(core.request_permission(&a.get::<String>("kind")?).map_err(e)?),
        "open_permission_settings" => ok(core.open_permission_settings(&a.get::<String>("kind")?).map_err(e)?),
        "complete_onboarding" => ok(core.complete_onboarding().map_err(e)?),
        "pause_meeting" => ok(core.pause_meeting(true).map_err(e)?),
        "resume_meeting" => ok(core.pause_meeting(false).map_err(e)?),
        "set_mic_muted" => {
            core.set_mic_muted(a.get("on")?);
            ok(())
        }
        "delete_segment" => ok(core.delete_segment(a.get::<i64>("id")?).map_err(e)?),
        "log_ui_error" => {
            tracing::error!(target: "ui", "{}", a.s("message").unwrap_or_default());
            ok(())
        }
        other => Err(format!("commande inconnue : {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn toutes_les_commandes_du_contrat_sont_connues() {
        let core = Core::new(crate::Db::open_in_memory().unwrap(), Default::default()).unwrap();
        // Le contrat (docs/contracts.md) doit lister chaque commande, et le répartiteur la connaître.
        let contract = include_str!("../../../docs/contracts.md");
        for c in COMMANDS {
            if *c != "log_ui_error" {
                assert!(contract.contains(&format!("`{c}`")), "{c} absente de docs/contracts.md");
            }
            // Arguments vides : erreur d'argument possible, mais jamais « commande inconnue ».
            if matches!(*c, "delete_all_data" | "start_meeting" | "stop_meeting" | "wake_test_record" | "detect_agents" | "doctor" | "refresh_connectors" | "test_agent" | "list_models") {
                continue;
            }
            if let Err(m) = dispatch(&core, c, json!({})).await {
                assert!(!m.contains("commande inconnue") && !m.contains("unknown command"), "{c} : {m}");
            }
        }
        let unknown = dispatch(&core, "nope", json!({})).await.unwrap_err();
        assert!(unknown.contains("inconnue") || unknown.contains("unknown command"), "{unknown}");
        let s = dispatch(&core, "get_settings", Value::Null).await.unwrap();
        assert_eq!(s["voice"]["wake_word"], "Sténo");
        let d = dispatch(&core, "wake_detect", json!({"text": "Sténo, arrête l'écoute"})).await.unwrap();
        assert_eq!(d["intent"], "stop");
    }
}
