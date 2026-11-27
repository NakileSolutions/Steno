//! Adaptateur Codex (`codex exec`). Bac à sable lecture seule pour les fichiers et le shell.
//!
//! Codex ne sait pas (à la version testée) interdire des outils MCP par motif : ses
//! connecteurs sont donc « non bloquables » et ne peuvent pas être activés (PLAN §5.4).
//! Les serveurs MCP configurés sont neutralisés à l'exécution (`-c mcp_servers={}`,
//! À VÉRIFIER au spike S3).

use super::*;
use crate::agents::process::run_streaming;
use serde_json::Value;

const BIN: &str = "codex";
const API_KEY_ENV: &[&str] = &["OPENAI_API_KEY", "CODEX_API_KEY"];

#[derive(Default)]
pub struct Codex;

pub(crate) fn build_args(req: &RunRequest, last_message: &Path) -> Vec<String> {
    let mut a: Vec<String> = vec!["exec".into()];
    if let SessionMode::Resume(id) = &req.session {
        a.push("resume".into());
        a.push(id.clone());
    }
    a.extend(
        [
            "--json",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "-c",
            "mcp_servers={}",
            "-c",
            "approval_policy=\"never\"",
        ]
        .map(String::from),
    );
    a.push("-C".into());
    a.push(req.run_dir.display().to_string());
    a.push("--output-last-message".into());
    a.push(last_message.display().to_string());
    if let Some(m) = &req.model {
        a.push("-m".into());
        a.push(m.clone());
    }
    // La valeur est insérée dans du TOML (`-c clé=valeur`) : liste blanche stricte.
    match &req.effort {
        Some(e) if crate::settings::valid_effort(e) => {
            a.push("-c".into());
            a.push(format!("model_reasoning_effort=\"{e}\""));
        }
        Some(e) => tracing::warn!("niveau de réflexion ignoré (caractères non autorisés) : {e:?}"),
        None => {}
    }
    // L'invite est lue sur l'entrée standard.
    a.push("-".into());
    a
}

fn codex_home() -> PathBuf {
    std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".codex"))
}

/// Modèles cités dans `config.toml` (clé `model` à la racine et dans les profils).
pub(crate) fn models_from_config(path: &Path) -> Vec<ModelInfo> {
    let Ok(text) = std::fs::read_to_string(path) else { return vec![] };
    let Ok(v) = toml::from_str::<toml::Value>(&text) else { return vec![] };
    let mut out: Vec<ModelInfo> = vec![];
    let mut add = |m: &str, note: String| {
        if !out.iter().any(|x| x.id == m) {
            out.push(ModelInfo { id: m.into(), name: m.into(), note: Some(note) });
        }
    };
    if let Some(m) = v.get("model").and_then(|m| m.as_str()) {
        add(m, "modèle de votre config Codex".into());
    }
    if let Some(p) = v.get("profiles").and_then(|p| p.as_table()) {
        for (name, prof) in p {
            if let Some(m) = prof.get("model").and_then(|m| m.as_str()) {
                add(m, format!("profil Codex « {name} »"));
            }
        }
    }
    out
}

#[derive(Default)]
pub(crate) struct JsonlParser {
    pub thread_id: Option<String>,
    pub messages: Vec<String>,
    pub tools: Vec<String>,
    pub error: Option<String>,
}

impl JsonlParser {
    pub fn line(&mut self, line: &str) -> Option<AgentEvent> {
        let v: Value = serde_json::from_str(line.trim()).ok()?;
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "thread.started" | "session.created" => {
                self.thread_id = v
                    .get("thread_id")
                    .or_else(|| v.get("session_id"))
                    .and_then(|t| t.as_str())
                    .map(String::from);
                return Some(AgentEvent::Started { session_id: self.thread_id.clone() });
            }
            "turn.failed" | "error" => {
                self.error = v
                    .pointer("/error/message")
                    .or_else(|| v.get("message"))
                    .and_then(|m| m.as_str())
                    .map(String::from);
                return None;
            }
            _ => {}
        }
        // Nouveau format : item.*
        if let Some(item) = v.get("item") {
            let it = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let done = ty == "item.completed";
            match it {
                "agent_message" | "assistant_message" if done => {
                    let t = item.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string();
                    self.messages.push(t.clone());
                    return Some(AgentEvent::Text(t));
                }
                "command_execution" if done => {
                    let c = item.get("command").and_then(|c| c.as_str()).unwrap_or("shell");
                    let name = format!("shell: {}", crate::text::truncate(c, 80));
                    self.tools.push(name.clone());
                    return Some(AgentEvent::ToolCall { name });
                }
                "mcp_tool_call" if done => {
                    let name = format!(
                        "mcp__{}__{}",
                        item.get("server").and_then(|s| s.as_str()).unwrap_or("?"),
                        item.get("tool").and_then(|s| s.as_str()).unwrap_or("?")
                    );
                    self.tools.push(name.clone());
                    return Some(AgentEvent::ToolCall { name });
                }
                _ => {}
            }
        }
        // Ancien format : {"msg":{"type":"agent_message","message":"…"}}
        if let Some(msg) = v.get("msg") {
            match msg.get("type").and_then(|t| t.as_str()) {
                Some("agent_message") => {
                    let t = msg.get("message").and_then(|t| t.as_str()).unwrap_or("").to_string();
                    self.messages.push(t.clone());
                    return Some(AgentEvent::Text(t));
                }
                Some("session_configured") => {
                    self.thread_id = msg.get("session_id").and_then(|t| t.as_str()).map(String::from);
                }
                Some("exec_command_begin") => {
                    self.tools.push("shell".into());
                }
                Some("mcp_tool_call_begin") => {
                    self.tools.push("mcp".into());
                }
                _ => {}
            }
        }
        None
    }
}

#[async_trait]
impl AgentAdapter for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn display_name(&self) -> &'static str {
        "Codex"
    }
    fn support(&self) -> Support {
        Support::Supported
    }
    async fn detect(&self) -> Detection {
        detect_binary(BIN).await
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { resume_session: true, stream_json: true, read_only_files: true, tool_deny_list: false, extra_dirs: true }
    }
    fn efforts(&self) -> Vec<&'static str> {
        // Clé `model_reasoning_effort` ; les niveaux réellement acceptés dépendent du modèle.
        vec!["low", "medium", "high", "xhigh", "max"]
    }
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Pas de commande de liste documentée : modèles cités dans la config de l'utilisateur.
        Ok(models_from_config(&codex_home().join("config.toml")))
    }
    async fn list_mcp(&self) -> Result<Vec<McpServer>> {
        run_mcp_list(BIN, &["mcp", "list"]).await
    }
    async fn sync_instructions(&self, run_dir: &Path, instructions: &Instructions) -> Result<()> {
        std::fs::create_dir_all(run_dir)?;
        std::fs::write(run_dir.join("AGENTS.md"), &instructions.markdown)?;
        Ok(())
    }
    async fn apply_policy(&self, _policy: &ReadOnlyPolicy, _run_dir: &Path) -> Result<()> {
        // Traduit à l'exécution : `--sandbox read-only`, MCP neutralisés.
        Ok(())
    }
    async fn run(&self, req: RunRequest, events: mpsc::Sender<AgentEvent>, cancel: CancellationToken) -> Result<RunResult> {
        if matches!(&req.tools, ToolAccess::ReadOnly { connectors } if !connectors.is_empty()) {
            anyhow::bail!("Codex ne sait pas bloquer les outils MCP en écriture : connecteurs refusés");
        }
        let bin = which::which(BIN).map_err(|_| anyhow::anyhow!("Codex (« codex ») introuvable"))?;
        std::fs::create_dir_all(req.run_dir.join(".steno"))?;
        let last = req.run_dir.join(".steno").join(format!("last-{}.md", req.run_id));
        let mut prompt = req.prompt.clone();
        if !req.repos.is_empty() && req.tools != ToolAccess::None {
            prompt.push_str("\n\nDépôts lisibles (lecture seule) :\n");
            for r in &req.repos {
                prompt.push_str(&format!("- {}\n", r.display()));
            }
        }
        if req.tools == ToolAccess::None {
            prompt.push_str("\n\nN'utilise aucun outil ni aucune commande : réponds directement.");
        }
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(build_args(&req, &last)).current_dir(&req.run_dir);
        strip_env(&mut cmd, req.allow_api_key, API_KEY_ENV);
        let mut parser = JsonlParser::default();
        let ev = events.clone();
        let out = run_streaming(cmd, Some(prompt), req.timeout, cancel, |line| {
            if let Some(e) = parser.line(line) {
                let _ = ev.try_send(e);
            }
        })
        .await?;
        let last_msg = std::fs::read_to_string(&last).ok();
        let _ = std::fs::remove_file(&last);
        if out.cancelled {
            anyhow::bail!("annulée");
        }
        if !out.success() {
            anyhow::bail!("{}", parser.error.clone().unwrap_or_else(|| out.error_message("Codex")));
        }
        let answer = last_msg
            .filter(|m| !m.trim().is_empty())
            .or_else(|| parser.messages.last().cloned())
            .unwrap_or_default();
        let session_id = parser.thread_id.clone().or(match &req.session {
            SessionMode::Resume(id) => Some(id.clone()),
            _ => None,
        });
        Ok(RunResult { answer, session_id, tools_used: parser.tools.clone(), model: req.model.clone(), denied_tools: vec![] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modeles_de_la_config() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        std::fs::write(&p, "model = \"gpt-x\"\n[profiles.rapide]\nmodel = \"gpt-y\"\n").unwrap();
        let m = models_from_config(&p);
        assert_eq!(m.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["gpt-x", "gpt-y"]);
    }

    #[test]
    fn analyse_jsonl() {
        let mut p = JsonlParser::default();
        for l in [
            r#"{"type":"thread.started","thread_id":"th-1"}"#,
            r#"{"type":"item.completed","item":{"id":"i1","type":"command_execution","command":"rg -n creneau","exit_code":0}}"#,
            r#"{"type":"item.completed","item":{"id":"i2","type":"agent_message","text":"TL;DR: ok"}}"#,
            r#"{"type":"turn.completed","usage":{}}"#,
        ] {
            p.line(l);
        }
        assert_eq!(p.thread_id.as_deref(), Some("th-1"));
        assert_eq!(p.messages, vec!["TL;DR: ok"]);
        assert_eq!(p.tools.len(), 1);
    }

    #[test]
    fn arguments_reprise() {
        let r = RunRequest {
            run_dir: PathBuf::from("/tmp/r"),
            prompt: String::new(),
            tier: Tier::Quick,
            model: None,
            effort: Some("high".into()),
            session: SessionMode::Resume("th-1".into()),
            repos: vec![],
            exclude_globs: vec![],
            tools: ToolAccess::None,
            max_tool_calls: 0,
            timeout: Duration::from_secs(5),
            allow_api_key: false,
            run_id: "x".into(),
            hook_exe: None,
            skills: false,
        };
        let a = build_args(&r, Path::new("/tmp/l"));
        assert_eq!(&a[..3], &["exec", "resume", "th-1"]);
        assert!(a.contains(&"read-only".to_string()));
        assert!(a.contains(&"model_reasoning_effort=\"high\"".to_string()));
        assert_eq!(a.last().unwrap(), "-");
        let mut r = r;
        for e in ["high\"\nmcp_servers.x.command=\"sh", "high\" sandbox_mode=\"danger-full-access", "x".repeat(21).as_str(), "HIGH"] {
            r.effort = Some(e.to_string());
            let a = build_args(&r, Path::new("/tmp/l"));
            assert!(!a.iter().any(|x| x.starts_with("model_reasoning_effort")), "{e:?} doit être ignoré");
            assert!(a.contains(&"mcp_servers={}".to_string()));
        }
    }
}
