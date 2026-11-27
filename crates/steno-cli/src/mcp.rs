//! `steno mcp` : serveur MCP local, stdio, **lecture seule** (PLAN §9).
//! JSON-RPC 2.0, un message par ligne. Les apartés ne sont jamais renvoyés.

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

pub fn call_tool(db: &Db, name: &str, a: &Value) -> Result<Value> {
    Ok(match name {
        "search_meetings" => {
            let q = arg(a, "query").unwrap_or("");
            let meetings = db.list_meetings(&MeetingFilter {
                query: Some(q.to_string()),
                from: arg(a, "from").map(String::from),
                to: arg(a, "to").map(String::from),
                limit: Some(20),
                ..Default::default()
            })?;
            let hits = db.search(q, 30)?;
            json!({
                "meetings": meetings.iter().map(|m| json!({
                    "id": m.meeting.id, "title": m.meeting.title, "started_at": m.meeting.started_at, "snippet": m.snippet
                })).collect::<Vec<_>>(),
                "hits": hits
            })
        }
        "get_meeting" => {
            let id = arg(a, "id").ok_or_else(|| anyhow::anyhow!("id manquant"))?;
            let mut d = serde_json::to_value(db.meeting_detail(id)?.ok_or_else(|| anyhow::anyhow!("réunion introuvable"))?)?;
            if let Some(m) = d.get_mut("meeting").and_then(|m| m.as_object_mut()) {
                m.remove("agent_session_id");
            }
            if let Some(o) = d.as_object_mut() {
                o.remove("report");
            }
            d
        }
        "get_transcript" => {
            let id = arg(a, "id").ok_or_else(|| anyhow::anyhow!("id manquant"))?;
            let from = arg(a, "from_ts").and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok());
            let to = arg(a, "to_ts").and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok());
            let segs: Vec<Value> = db
                .segments(id, false, false)?
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
            let id = arg(a, "id").ok_or_else(|| anyhow::anyhow!("id manquant"))?;
            match db.get_report(id)? {
                Some(r) => json!({"id": id, "version": r.version, "correlated": r.correlated, "markdown": r.markdown}),
                None => json!({"id": id, "markdown": null, "message": "pas encore de compte rendu"}),
            }
        }
        "list_decisions" => serde_json::to_value(db.list_decisions(arg(a, "since"))?)?,
        "list_actions" => serde_json::to_value(db.list_actions(&ActionFilter {
            status: arg(a, "status").map(String::from),
            owner: arg(a, "owner").map(String::from),
            ..Default::default()
        })?)?,
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
        assert_eq!(r["result"]["isError"], false);
        let r = handle(&db, &json!({"jsonrpc":"2.0","id":5,"method":"nope"})).unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }
}
