//! Adaptateur OpenCode (`opencode run`), expérimental.
//!
//! Lecture seule : `opencode.json` dans le dossier d'exécution refuse l'édition, le web et
//! toute commande shell hors de la liste autorisée. Les outils MCP ne peuvent pas être
//! filtrés de façon fiable : connecteurs non activables. À VÉRIFIER (S3).

use super::*;
use crate::agents::process::run_streaming;
use serde_json::{json, Value};

const BIN: &str = "opencode";
const API_KEY_ENV: &[&str] = &["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY", "OPENROUTER_API_KEY"];

#[derive(Default)]
pub struct OpenCode;

pub(crate) fn config(policy: &ReadOnlyPolicy) -> Value {
    let mut bash = serde_json::Map::new();
    for c in &policy.base.allowed_commands {
        bash.insert(format!("{c} *"), json!("allow"));
        bash.insert(c.clone(), json!("allow"));
    }
    bash.insert("*".into(), json!("deny"));
    json!({
        "$schema": "https://opencode.ai/config.json",
        "permission": { "edit": "deny", "webfetch": "deny", "bash": bash },
        "tools": { "write": false, "edit": false, "patch": false, "webfetch": false }
    })
}

/// Extrait la réponse : JSON en flux (parties `text`) ou texte brut.
pub(crate) fn parse_output(stdout: &str) -> (String, Option<String>, Vec<String>) {
    let mut texts = vec![];
    let mut session = None;
    let mut tools = vec![];
    let mut json_seen = false;
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
        json_seen = true;
        if let Some(s) = v.get("sessionID").or_else(|| v.get("session_id")).and_then(|s| s.as_str()) {
            session = Some(s.to_string());
        }
        let part = v.get("part").unwrap_or(&v);
        match part.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                    texts.push(t.to_string());
                }
            }
            Some("tool") => {
                if let Some(t) = part.get("tool").and_then(|t| t.as_str()) {
                    tools.push(t.to_string());
                }
            }
            _ => {}
        }
    }
    if !json_seen {
        return (stdout.trim().to_string(), None, vec![]);
    }
    (texts.join("\n"), session, tools)
}

#[async_trait]
impl AgentAdapter for OpenCode {
    fn id(&self) -> &'static str {
        "opencode"
    }
    fn display_name(&self) -> &'static str {
        "OpenCode"
    }
    fn support(&self) -> Support {
        Support::Experimental
    }
    async fn detect(&self) -> Detection {
        detect_binary(BIN).await
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { resume_session: true, stream_json: true, read_only_files: true, tool_deny_list: false, extra_dirs: false }
    }
    async fn list_mcp(&self) -> Result<Vec<McpServer>> {
        run_mcp_list(BIN, &["mcp", "list"]).await
    }
    async fn sync_instructions(&self, run_dir: &Path, instructions: &Instructions) -> Result<()> {
        std::fs::create_dir_all(run_dir)?;
        std::fs::write(run_dir.join("AGENTS.md"), &instructions.markdown)?;
        Ok(())
    }
    async fn apply_policy(&self, policy: &ReadOnlyPolicy, run_dir: &Path) -> Result<()> {
        std::fs::write(run_dir.join("opencode.json"), serde_json::to_vec_pretty(&config(policy))?)?;
        Ok(())
    }
    async fn run(&self, req: RunRequest, events: mpsc::Sender<AgentEvent>, cancel: CancellationToken) -> Result<RunResult> {
        if matches!(&req.tools, ToolAccess::ReadOnly { connectors } if !connectors.is_empty()) {
            anyhow::bail!("OpenCode ne sait pas bloquer les outils MCP en écriture : connecteurs refusés");
        }
        let bin = which::which(BIN).map_err(|_| anyhow::anyhow!("OpenCode (« opencode ») introuvable"))?;
        let mut a: Vec<String> = vec!["run".into(), "--format".into(), "json".into()];
        if let Some(m) = &req.model {
            a.push("-m".into());
            a.push(m.clone());
        }
        if let SessionMode::Resume(id) = &req.session {
            a.push("--session".into());
            a.push(id.clone());
        }
        let mut prompt = req.prompt.clone();
        if req.tools == ToolAccess::None {
            prompt.push_str("\n\nN'utilise aucun outil : réponds directement.");
        } else if !req.repos.is_empty() {
            prompt.push_str("\n\nDépôts lisibles :\n");
            for r in &req.repos {
                prompt.push_str(&format!("- {}\n", r.display()));
            }
        }
        a.push(prompt);
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(a).current_dir(&req.run_dir);
        strip_env(&mut cmd, req.allow_api_key, API_KEY_ENV);
        let ev = events.clone();
        let out = run_streaming(cmd, None, req.timeout, cancel, |l| {
            if l.contains("\"tool\"") {
                let _ = ev.try_send(AgentEvent::ToolCall { name: "outil".into() });
            }
        })
        .await?;
        if out.cancelled {
            anyhow::bail!("annulée");
        }
        if !out.success() {
            anyhow::bail!("{}", out.error_message("OpenCode"));
        }
        let (answer, session, tools) = parse_output(&out.stdout);
        Ok(RunResult { answer, session_id: session, tools_used: tools, model: req.model.clone(), denied_tools: vec![] })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sortie() {
        let (a, s, t) = super::parse_output(
            "{\"type\":\"text\",\"sessionID\":\"ses_1\",\"part\":{\"type\":\"text\",\"text\":\"Bonjour\"}}\n{\"type\":\"tool_use\",\"part\":{\"type\":\"tool\",\"tool\":\"grep\"}}\n",
        );
        assert_eq!(a, "Bonjour");
        assert_eq!(s.as_deref(), Some("ses_1"));
        assert_eq!(t, vec!["grep"]);
        assert_eq!(super::parse_output("brut").0, "brut");
    }
}
