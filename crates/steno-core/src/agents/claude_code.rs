//! Adaptateur Claude Code (`claude -p`), référence. Options vérifiées sur la v2.1.x
//! (voir docs/adapters.md).
//!
//! Lecture seule : mode de permission `dontAsk` (tout ce qui n'est pas explicitement
//! autorisé est refusé), liste d'outils autorisés minimale, refus explicites des outils
//! d'écriture, et crochet `PreToolUse` qui applique `ReadOnlyPolicy` (motifs MCP interdits,
//! shell commands, paths, call budget). Without a hook executable, Claude Code is not
//! started.

use super::*;
use crate::agents::process::run_streaming;
use serde_json::{json, Value};
use std::collections::HashMap;

const BIN: &str = "claude";
const API_KEY_ENV: &[&str] = &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

#[derive(Default)]
pub struct ClaudeCode;

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// `--settings` of a run: explicit denials + policy hook. Without a hook executable, the run
/// is refused: the policy (commands, paths, budget, MCP tools) would not be
/// enforced.
pub(crate) fn run_settings(req: &RunRequest) -> Result<Value> {
    let Some(exe) = &req.hook_exe else {
        anyhow::bail!("crochet de politique indisponible : exécution de Claude Code refusée");
    };
    let cmd = format!(
        "{} policy-hook {}",
        shell_quote(&exe.display().to_string()),
        shell_quote(&policy_file(&req.run_dir, &req.run_id).display().to_string())
    );
    Ok(json!({
        "permissions": {
            "deny": ["Write", "Edit", "MultiEdit", "NotebookEdit", "WebFetch", "WebSearch", "Task"],
        },
        "hooks": {
            "PreToolUse": [{ "matcher": "*", "hooks": [{ "type": "command", "command": cmd, "timeout": 30 }] }]
        },
    }))
}

/// Arguments de la ligne de commande pour une exécution.
pub(crate) fn build_args(req: &RunRequest, settings_path: &Path, policy: &ReadOnlyPolicy) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-p".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
        "--permission-mode".into(),
        "dontAsk".into(),
        "--settings".into(),
        settings_path.display().to_string(),
    ];
    if let Some(m) = &req.model {
        a.push("--model".into());
        a.push(m.clone());
    }
    if let Some(e) = &req.effort {
        a.push("--effort".into());
        a.push(e.clone());
    }
    match &req.session {
        SessionMode::None => a.push("--no-session-persistence".into()),
        SessionMode::New(id) => {
            a.push("--session-id".into());
            a.push(id.clone());
        }
        SessionMode::Resume(id) => {
            a.push("--resume".into());
            a.push(id.clone());
        }
    }
    match &req.tools {
        ToolAccess::None => {
            a.push("--tools".into());
            a.push(String::new());
            a.push("--strict-mcp-config".into());
            a.push("--mcp-config".into());
            a.push(r#"{"mcpServers":{}}"#.into());
        }
        ToolAccess::ReadOnly { connectors } => {
            let mut builtins = policy.base.builtin_read_tools.clone();
            builtins.retain(|t| t != "LS");
            builtins.push("Bash".into());
            if policy.allow_skills {
                builtins.push("Skill".into());
            }
            a.push("--tools".into());
            a.push(builtins.join(","));
            let mut allowed: Vec<String> = policy.base.builtin_read_tools.iter().filter(|t| *t != "LS").cloned().collect();
            for c in &policy.base.allowed_commands {
                allowed.push(format!("Bash({c} *)"));
                allowed.push(format!("Bash({c}:*)"));
            }
            for c in connectors {
                allowed.push(format!("mcp__{}", crate::policy::mcp_server_id(c)));
            }
            if policy.allow_skills {
                allowed.push("Skill".into());
            }
            a.push("--allowedTools".into());
            a.extend(allowed);
            a.push("--disallowedTools".into());
            a.extend(["Write", "Edit", "MultiEdit", "NotebookEdit", "WebFetch", "WebSearch", "Task"].map(String::from));
            if connectors.is_empty() {
                a.push("--strict-mcp-config".into());
                a.push("--mcp-config".into());
                a.push(r#"{"mcpServers":{}}"#.into());
            }
            for r in &req.repos {
                a.push("--add-dir".into());
                a.push(r.display().to_string());
            }
        }
    }
    a
}

/// État d'analyse de la sortie `stream-json`.
#[derive(Default)]
pub(crate) struct StreamParser {
    pub result: Option<String>,
    pub is_error: bool,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub tools_used: Vec<String>,
    pub denied: Vec<String>,
    pub texts: Vec<String>,
    tool_ids: HashMap<String, String>,
}

impl StreamParser {
    /// Analyse une ligne ; renvoie un événement éventuel.
    pub fn line(&mut self, line: &str) -> Option<AgentEvent> {
        let v: Value = serde_json::from_str(line.trim()).ok()?;
        if let Some(s) = v.get("session_id").and_then(|s| s.as_str()) {
            self.session_id = Some(s.to_string());
        }
        match v.get("type").and_then(|t| t.as_str())? {
            "system" if v.get("subtype").and_then(|s| s.as_str()) == Some("init") => {
                self.model = v.get("model").and_then(|m| m.as_str()).map(String::from);
                Some(AgentEvent::Started { session_id: self.session_id.clone() })
            }
            "assistant" => {
                let mut ev = None;
                for c in v.pointer("/message/content").and_then(|c| c.as_array()).into_iter().flatten() {
                    match c.get("type").and_then(|t| t.as_str()) {
                        Some("tool_use") => {
                            let name = c.get("name").and_then(|n| n.as_str()).unwrap_or("?").to_string();
                            if let Some(id) = c.get("id").and_then(|i| i.as_str()) {
                                self.tool_ids.insert(id.to_string(), name.clone());
                            }
                            self.tools_used.push(name.clone());
                            ev = Some(AgentEvent::ToolCall { name });
                        }
                        Some("text") => {
                            if let Some(t) = c.get("text").and_then(|t| t.as_str()) {
                                self.texts.push(t.to_string());
                                ev = Some(AgentEvent::Text(t.to_string()));
                            }
                        }
                        _ => {}
                    }
                }
                ev
            }
            "user" => {
                for c in v.pointer("/message/content").and_then(|c| c.as_array()).into_iter().flatten() {
                    if c.get("type").and_then(|t| t.as_str()) == Some("tool_result")
                        && c.get("is_error").and_then(|e| e.as_bool()) == Some(true)
                    {
                        let content = c.get("content").map(|x| x.to_string()).unwrap_or_default();
                        if content.contains("Sténo") || content.to_lowercase().contains("permission") || content.contains("denied") {
                            if let Some(name) = c.get("tool_use_id").and_then(|i| i.as_str()).and_then(|i| self.tool_ids.get(i)) {
                                self.denied.push(name.clone());
                            }
                        }
                    }
                }
                None
            }
            "result" => {
                self.is_error = v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false)
                    || v.get("subtype").and_then(|s| s.as_str()).map(|s| s.starts_with("error")).unwrap_or(false);
                self.result = v.get("result").and_then(|r| r.as_str()).map(String::from);
                if self.denied.is_empty() {
                    for d in v.get("permission_denials").and_then(|d| d.as_array()).into_iter().flatten() {
                        if let Some(n) = d.get("tool_name").and_then(|n| n.as_str()) {
                            self.denied.push(n.to_string());
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Outils réellement exécutés (les refusés sont retirés).
    pub fn executed_tools(&self) -> Vec<String> {
        let mut denied = self.denied.clone();
        let mut out = vec![];
        for t in &self.tools_used {
            if let Some(p) = denied.iter().position(|d| d == t) {
                denied.remove(p);
            } else {
                out.push(t.clone());
            }
        }
        out
    }
}

#[async_trait]
impl AgentAdapter for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    fn display_name(&self) -> &'static str {
        "Claude Code"
    }
    fn support(&self) -> Support {
        Support::Supported
    }
    async fn detect(&self) -> Detection {
        detect_binary(BIN).await
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { resume_session: true, stream_json: true, read_only_files: true, tool_deny_list: true, extra_dirs: true }
    }
    fn efforts(&self) -> Vec<&'static str> {
        // `claude --help` (v2.1.x) : --effort <level> (low, medium, high, xhigh, max).
        vec!["low", "medium", "high", "xhigh", "max"]
    }
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Pas de commande de liste : alias (toujours la dernière version) et identifiants connus.
        let alias = Some("alias : dernière version");
        Ok(vec![
            ModelInfo::new("fable", "Fable", alias),
            ModelInfo::new("opus", "Opus", alias),
            ModelInfo::new("sonnet", "Sonnet", alias),
            ModelInfo::new("haiku", "Haiku", alias),
            ModelInfo::new("claude-fable-5-2", "Claude Fable 5.2", None),
            ModelInfo::new("claude-opus-5-5", "Claude Opus 5.5", None),
            ModelInfo::new("claude-sonnet-5-5", "Claude Sonnet 5.5", None),
            ModelInfo::new("claude-haiku-5-5", "Claude Haiku 5.5", None),
        ])
    }
    async fn list_mcp(&self) -> Result<Vec<McpServer>> {
        run_mcp_list(BIN, &["mcp", "list"]).await
    }
    async fn sync_instructions(&self, run_dir: &Path, instructions: &Instructions) -> Result<()> {
        std::fs::create_dir_all(run_dir)?;
        std::fs::write(run_dir.join("CLAUDE.md"), &instructions.markdown)?;
        Ok(())
    }
    async fn apply_policy(&self, _policy: &ReadOnlyPolicy, run_dir: &Path) -> Result<()> {
        // Réglages de projet : refus de base, même si `--settings` était ignoré.
        let dir = run_dir.join(".claude");
        std::fs::create_dir_all(&dir)?;
        let s = json!({
            "permissions": {
                "deny": ["Write", "Edit", "MultiEdit", "NotebookEdit", "WebFetch", "WebSearch", "Task"],
                "defaultMode": "dontAsk"
            }
        });
        std::fs::write(dir.join("settings.json"), serde_json::to_vec_pretty(&s)?)?;
        Ok(())
    }
    async fn run(&self, req: RunRequest, events: mpsc::Sender<AgentEvent>, cancel: CancellationToken) -> Result<RunResult> {
        let bin = which::which(BIN).map_err(|_| anyhow::anyhow!("Claude Code (« claude ») introuvable"))?;
        let settings = run_settings(&req)?;
        let policy = write_run_policy(&req)?;
        let settings_path = req.run_dir.join(".steno").join(format!("settings-{}.json", req.run_id));
        crate::paths::write_private(&settings_path, &serde_json::to_vec_pretty(&settings)?)?;
        let args = build_args(&req, &settings_path, &policy);
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(&args).current_dir(&req.run_dir);
        strip_env(&mut cmd, req.allow_api_key, API_KEY_ENV);
        let mut parser = StreamParser::default();
        let ev = events.clone();
        let out = run_streaming(cmd, Some(req.prompt.clone()), req.timeout, cancel, |line| {
            if let Some(e) = parser.line(line) {
                let _ = ev.try_send(e);
            }
        })
        .await;
        cleanup_run(&req);
        let out = out?;
        if out.cancelled {
            anyhow::bail!("annulée");
        }
        if !out.success() || parser.is_error {
            let msg = parser.result.clone().filter(|r| !r.is_empty()).unwrap_or_else(|| out.error_message("Claude Code"));
            anyhow::bail!("{msg}");
        }
        let answer = parser.result.clone().unwrap_or_else(|| parser.texts.join("\n"));
        Ok(RunResult {
            answer,
            session_id: parser.session_id.clone(),
            tools_used: parser.executed_tools(),
            model: parser.model.clone(),
            denied_tools: parser.denied.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(tools: ToolAccess) -> RunRequest {
        RunRequest {
            run_dir: PathBuf::from("/tmp/run"),
            prompt: "x".into(),
            tier: Tier::Search,
            model: Some("modele-x".into()),
            effort: Some("high".into()),
            session: SessionMode::Resume("abc".into()),
            repos: vec![PathBuf::from("/src/app")],
            exclude_globs: vec![],
            tools,
            max_tool_calls: 15,
            timeout: Duration::from_secs(10),
            allow_api_key: false,
            run_id: "r1".into(),
            hook_exe: Some(PathBuf::from("/usr/local/bin/steno")),
            skills: false,
        }
    }

    #[test]
    fn arguments() {
        let r = req(ToolAccess::ReadOnly { connectors: vec!["claude.ai Atlassian".into()] });
        let p = ReadOnlyPolicy::no_tools();
        let a = build_args(&r, Path::new("/tmp/s.json"), &p);
        let s = a.join(" ");
        assert!(s.contains("--permission-mode dontAsk"));
        assert!(s.contains("--resume abc"));
        assert!(s.contains("--model modele-x"));
        assert!(s.contains("--effort high"));
        assert!(s.contains("mcp__claude_ai_Atlassian"));
        assert!(s.contains("--add-dir /src/app"));
        assert!(!s.contains("--strict-mcp-config"));
        assert!(!a.iter().any(|x| x == "Skill" || x.contains(",Skill")));
        let mut ps = ReadOnlyPolicy::no_tools();
        ps.allow_skills = true;
        let a = build_args(&r, Path::new("/tmp/s.json"), &ps);
        assert!(a.windows(2).any(|w| w[0] == "--tools" && w[1].ends_with(",Skill")));
        assert!(a.contains(&"Skill".to_string()));
        let a = build_args(&req(ToolAccess::None), Path::new("/tmp/s.json"), &p);
        assert!(a.windows(2).any(|w| w[0] == "--tools" && w[1].is_empty()));
        assert!(a.contains(&"--strict-mcp-config".to_string()));
        let st = run_settings(&r).unwrap();
        assert!(st["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str().unwrap().contains("policy-hook"));
    }

    #[test]
    fn no_run_without_hook() {
        let mut r = req(ToolAccess::ReadOnly { connectors: vec![] });
        r.hook_exe = None;
        assert!(run_settings(&r).is_err(), "the policy would not be enforced");
    }

    #[test]
    fn analyse_stream_json() {
        let mut p = StreamParser::default();
        let lines = [
            r#"{"type":"system","subtype":"init","session_id":"s-1","model":"m-1","tools":["Read"]}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__claude_ai_Atlassian__searchJiraIssuesUsingJql","input":{}}]},"session_id":"s-1"}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"mcp__slack__send_message","input":{}}]},"session_id":"s-1"}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","is_error":true,"content":"Refusé par Sténo (lecture seule)"}]}}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"TL;DR: trouvé.","session_id":"s-1"}"#,
        ];
        for l in lines {
            p.line(l);
        }
        assert_eq!(p.session_id.as_deref(), Some("s-1"));
        assert_eq!(p.result.as_deref(), Some("TL;DR: trouvé."));
        assert_eq!(p.executed_tools(), vec!["mcp__claude_ai_Atlassian__searchJiraIssuesUsingJql"]);
        assert_eq!(p.denied, vec!["mcp__slack__send_message"]);
    }
}
