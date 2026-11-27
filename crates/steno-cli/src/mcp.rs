//! `steno mcp` : serveur MCP local, stdio, **lecture seule** (PLAN §9).
//! JSON-RPC 2.0, un message par ligne. Les apartés (transcription privée et demandes posées en
//! aparté) et les réunions confidentielles ne sont jamais renvoyés.

use anyhow::Result;
use steno_core::model::{ActionFilter, MeetingFilter};
use steno_core::Db;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const PROTOCOL: &str = "2025-06-18";

fn tools() -> Value {
    let s = |props: Value, req: &[&str]| json!({"type": "object", "properties": props, "required": req});
    json!([
        {"name": "search_meetings", "description": "Recherche plein texte dans les transcriptions et comptes rendus des réunions. Renvoie les réunions et des extraits.",
         "inputSchema": s(json!({"query": {"type": "string"}, "from": {"type": "string", "description": "date ISO (incluse)"}, "to": {"type": "string", "description": "date ISO (incluse)"}}), &["query"]),
         "annotations": {"readOnlyHint": true}},
        {"name": "get_meeting", "description": "Détails d'une réunion : titre, dates, intervenants, notes, demandes, décisions, actions.",
         "inputSchema": s(json!({"id": {"type": "string"}}), &["id"]), "annotations": {"readOnlyHint": true}},
        {"name": "get_transcript", "description": "Transcription d'une réunion (sans les apartés), éventuellement entre deux horodatages ISO.",
         "inputSchema": s(json!({"id": {"type": "string"}, "from_ts": {"type": "string"}, "to_ts": {"type": "string"}}), &["id"]),
         "annotations": {"readOnlyHint": true}},
        {"name": "get_report", "description": "Compte rendu (markdown) d'une réunion.",
         "inputSchema": s(json!({"id": {"type": "string"}}), &["id"]), "annotations": {"readOnlyHint": true}},
        {"name": "list_decisions", "description": "Décisions prises en réunion, les plus récentes d'abord.",
         "inputSchema": s(json!({"since": {"type": "string", "description": "date ISO"}}), &[]), "annotations": {"readOnlyHint": true}},
        {"name": "list_actions", "description": "Actions décidées en réunion (qui / quoi / échéance).",
         "inputSchema": s(json!({"status": {"type": "string", "enum": ["open", "done"]}, "owner": {"type": "string"}}), &[]),
         "annotations": {"readOnlyHint": true}},
        {"name": "list_report_templates", "description": "Formats de compte rendu de l'utilisateur (prédéfinis puis perso) : nom, description et consignes de rédaction, pour réécrire un compte rendu dans le format voulu.",
         "inputSchema": s(json!({}), &[]), "annotations": {"readOnlyHint": true}}
    ])
}

fn arg<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

/// Réunion consultable : existante et non confidentielle. Même réponse dans les deux cas de
/// refus, pour ne pas révéler l'existence d'une réunion confidentielle.
fn public_meeting(db: &Db, a: &Value) -> Result<String> {
    let id = arg(a, "id").ok_or_else(|| anyhow::anyhow!("id manquant"))?;
    match db.get_meeting(id)? {
        Some(m) if !m.confidential => Ok(id.to_string()),
        _ => anyhow::bail!("réunion introuvable"),
    }
}

pub fn call_tool(db: &Db, name: &str, a: &Value) -> Result<Value> {
    Ok(match name {
        "search_meetings" => {
            let q = arg(a, "query").unwrap_or("");
            // Réunions confidentielles retirées avant la limite de 20 : elles ne masquent jamais
            // les réunions publiques plus anciennes.
            let meetings = db.list_meetings(&MeetingFilter {
                query: Some(q.to_string()),
                from: arg(a, "from").map(String::from),
                to: arg(a, "to").map(String::from),
                limit: Some(500),
                ..Default::default()
            })?;
            let hits = db.search_public(q, 30)?;
            json!({
                "meetings": meetings.iter().filter(|m| !m.meeting.confidential).take(20).map(|m| json!({
                    "id": m.meeting.id, "title": m.meeting.title, "started_at": m.meeting.started_at, "snippet": m.snippet
                })).collect::<Vec<_>>(),
                "hits": hits
            })
        }
        "get_meeting" => {
            let id = public_meeting(db, a)?;
            let mut detail = db.meeting_detail(&id)?.ok_or_else(|| anyhow::anyhow!("réunion introuvable"))?;
            detail.requests.retain(|r| r.origin != "hotkey");
            let mut d = serde_json::to_value(detail)?;
            if let Some(m) = d.get_mut("meeting").and_then(|m| m.as_object_mut()) {
                m.remove("agent_session_id");
            }
            if let Some(o) = d.as_object_mut() {
                o.remove("report");
            }
            d
        }
        "get_transcript" => {
            let id = public_meeting(db, a)?;
            let from = arg(a, "from_ts").and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok());
            let to = arg(a, "to_ts").and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok());
            let segs: Vec<Value> = db
                .segments(&id, false, false)?
                .into_iter()
                .filter(|s| {
                    let t = chrono::DateTime::parse_from_rfc3339(&s.ts).ok();
                    match t {
                        Some(t) => from.map(|f| t >= f).unwrap_or(true) && to.map(|x| t <= x).unwrap_or(true),
                        None => true,
                    }
                })
                .map(|s| json!({"ts": s.ts, "speaker": s.speaker, "text": s.text}))
                .collect();
            json!({"id": id, "segments": segs})
        }
        "get_report" => {
            let id = public_meeting(db, a)?;
            match db.get_report(&id)? {
                Some(r) => json!({"id": id, "version": r.version, "correlated": r.correlated, "markdown": r.markdown}),
                None => json!({"id": id, "markdown": null, "message": "pas encore de compte rendu"}),
            }
        }
        "list_decisions" => {
            let hidden = db.confidential_meeting_ids()?;
            let mut v = db.list_decisions(arg(a, "since"))?;
            v.retain(|d| !hidden.contains(&d.meeting_id));
            serde_json::to_value(v)?
        }
        "list_actions" => {
            let hidden = db.confidential_meeting_ids()?;
            let mut v = db.list_actions(&ActionFilter {
                status: arg(a, "status").map(String::from),
                owner: arg(a, "owner").map(String::from),
                ..Default::default()
            })?;
            v.retain(|x| !hidden.contains(&x.meeting_id));
            serde_json::to_value(v)?
        }
        "list_report_templates" => {
            let s = steno_core::settings::Settings::load(db)?;
            json!({"default": s.report.default_template, "templates": steno_core::report::templates(&s)})
        }
        other => anyhow::bail!("outil inconnu : {other}"),
    })
}

/// Traite un message JSON-RPC ; renvoie la réponse (ou rien pour une notification).
pub fn handle(db: &Db, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = id?; // notification : pas de réponse
    let result: Result<Value, (i64, String)> = match method {
        "initialize" => {
            let pv = msg.pointer("/params/protocolVersion").and_then(|v| v.as_str()).unwrap_or(PROTOCOL);
            Ok(json!({
                "protocolVersion": pv,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "steno", "title": "Sténo — réunions", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Accès en lecture seule aux réunions transcrites par Sténo (comptes rendus, décisions, actions, transcriptions sans apartés)."
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = msg.pointer("/params/name").and_then(|n| n.as_str()).unwrap_or("");
            let args = msg.pointer("/params/arguments").cloned().unwrap_or(json!({}));
            match call_tool(db, name, &args) {
                Ok(v) => Ok(json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default()}],
                                   "structuredContent": v, "isError": false})),
                Err(e) => Ok(json!({"content": [{"type": "text", "text": format!("Erreur : {e}")}], "isError": true})),
            }
        }
        "resources/list" => Ok(json!({"resources": []})),
        "prompts/list" => Ok(json!({"prompts": []})),
        _ => Err((-32601, format!("méthode inconnue : {method}"))),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err((code, m)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": m}}),
    })
}

pub fn serve(db: Db) -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let r: Vec<Value> = batch.iter().filter_map(|m| handle(&db, m)).collect();
                (!r.is_empty()).then_some(Value::Array(r))
            }
            Ok(msg) => handle(&db, &msg),
            Err(_) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "JSON invalide"}})),
        };
        if let Some(r) = resp {
            writeln!(stdout, "{}", serde_json::to_string(&r)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocole_mcp() {
        let db = Db::open_in_memory().unwrap();
        let r = handle(&db, &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}})).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert!(handle(&db, &json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let r = handle(&db, &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
        assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 7);
        let r = handle(&db, &json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"list_report_templates","arguments":{}}})).unwrap();
        assert_eq!(r["result"]["isError"], false);
        let tpl = &r["result"]["structuredContent"];
        assert_eq!(tpl["default"], "standard");
        assert!(tpl["templates"].as_array().unwrap().iter().any(|t| t["id"] == "mail" && t["instructions"].as_str().unwrap().len() > 20));
        let r = handle(&db, &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_actions","arguments":{}}})).unwrap();
        assert_eq!(r["result"]["isError"], false);
        let r = handle(&db, &json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_report","arguments":{"id":"x"}}})).unwrap();
        assert_eq!(r["result"]["isError"], true, "réunion inconnue");
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("réunion introuvable"));
        let r = handle(&db, &json!({"jsonrpc":"2.0","id":5,"method":"nope"})).unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }

    fn meeting(id: &str, title: &str) -> steno_core::model::Meeting {
        steno_core::model::Meeting {
            id: id.into(),
            title: Some(title.into()),
            title_source: None,
            started_at: "2026-11-26T10:30:00+01:00".into(),
            ended_at: None,
            status: "done".into(),
            confidential: false,
            agent_provider: None,
            agent_session_id: None,
            correlation: None,
            call_app: None,
        }
    }

    #[test]
    fn recherche_sans_masquage_par_les_confidentielles() {
        let db = Db::open_in_memory().unwrap();
        let mut old = meeting("ancienne", "Revue Brindille");
        old.started_at = "2026-01-05T10:00:00+01:00".into();
        db.insert_meeting(&old).unwrap();
        for i in 0..25 {
            let mut m = meeting(&format!("conf-{i}"), &format!("Brindille confidentielle {i}"));
            m.started_at = format!("2026-11-{:02}T10:00:00+01:00", i + 1);
            db.insert_meeting(&m).unwrap();
            db.set_confidential(&m.id, true).unwrap();
        }
        let found = call_tool(&db, "search_meetings", &json!({"query": "Brindille"})).unwrap();
        let ids: Vec<&str> = found["meetings"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["ancienne"], "25 réunions confidentielles récentes ne masquent pas la réunion publique");
    }

    #[test]
    fn reunions_confidentielles_et_apartes_absents() {
        let db = Db::open_in_memory().unwrap();
        for (id, title) in [("pub", "Daily ORION"), ("conf", "Point RH")] {
            db.insert_meeting(&meeting(id, title)).unwrap();
            db.insert_segment(&steno_core::db::NewSegment {
                meeting_id: id.into(),
                ts: steno_core::now_iso(),
                t0: Some(1.0),
                t1: Some(2.0),
                channel: "them".into(),
                speaker: "Eux".into(),
                text: format!("Brindille dans {title}"),
                private: false,
                is_echo: false,
            })
            .unwrap();
            db.save_report(id, &format!("# {title}\n\nBrindille"), None, None, false).unwrap();
            db.replace_decisions_actions(id, &[format!("décision {title}")], &[(Some("Marie".into()), format!("action {title}"), None)]).unwrap();
        }
        db.set_confidential("conf", true).unwrap();
        let call = |name: &str, args: Value| call_tool(&db, name, &args);
        let found = call("search_meetings", json!({"query": "Brindille"})).unwrap().to_string();
        assert!(found.contains("Daily ORION") && !found.contains("Point RH"), "{found}");
        for tool in ["get_meeting", "get_transcript", "get_report"] {
            let conf = call(tool, json!({"id": "conf"})).unwrap_err().to_string();
            let absent = call(tool, json!({"id": "n-existe-pas"})).unwrap_err().to_string();
            assert_eq!(conf, absent, "{tool} : aucune différence entre confidentielle et inexistante");
            assert_eq!(conf, "réunion introuvable");
            assert!(call(tool, json!({"id": "pub"})).is_ok(), "{tool}");
        }
        let d = call("list_decisions", json!({})).unwrap().to_string();
        assert!(d.contains("Daily ORION") || d.contains("décision Daily"), "{d}");
        assert!(!d.contains("Point RH"), "{d}");
        let a = call("list_actions", json!({})).unwrap().to_string();
        assert!(a.contains("action Daily ORION") && !a.contains("action Point RH"), "{a}");

        let q = |origin: &str, query: &str| steno_core::model::Request {
            id: format!("r-{origin}"),
            meeting_id: Some("pub".into()),
            created_at: steno_core::now_iso(),
            origin: origin.into(),
            intent: "search".into(),
            query: query.into(),
            context_seconds: None,
            tier: "search".into(),
            status: "done".into(),
            started_at: None,
            finished_at: None,
            error: None,
            summary: None,
            answer_md: None,
            tools_used: vec![],
        };
        db.insert_request(&q("voice", "où est le verrou de créneau")).unwrap();
        db.insert_request(&q("hotkey", "question en aparté sur l'entretien annuel")).unwrap();
        let m = call("get_meeting", json!({"id": "pub"})).unwrap().to_string();
        assert!(m.contains("verrou de créneau"), "{m}");
        assert!(!m.contains("entretien annuel"), "les demandes en aparté ne sortent jamais : {m}");
    }
}
