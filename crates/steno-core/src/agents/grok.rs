//! Adaptateur Grok Build CLI (`grok -p`), expérimental.
//! Doc : https://docs.x.ai/build/cli/reference et /build/cli/headless-scripting.
//!
//! - Non interactif : `grok -p "<invite>" --output-format json --no-auto-update`.
//! - Modèle : `-m <modèle>` (liste : `grok models`) ; réflexion : `--effort <niveau>` (niveaux
//!   selon le modèle : le CLI refuse un niveau non pris en charge).
//! - Session : `-s <uuid>` crée ou reprend une session non interactive nommée.
//! - Consignes : `--rules <texte>`.
//! - Lecture seule : outils intégrés d'écriture, web et MCP interdits (`--disallowed-tools`),
//!   recherche web et sous-agents désactivés. Pas de filtre fiable par motif sur les outils MCP :
//!   connecteurs non activables. Forme exacte de la sortie JSON et syntaxe de
//!   `--disallowed-tools` : À VÉRIFIER (S3).

use super::*;
use crate::agents::process::run_streaming;
use serde_json::Value;

const BIN: &str = "grok";
const API_KEY_ENV: &[&str] = &["XAI_API_KEY"];

#[derive(Default)]
pub struct Grok;

pub(crate) fn build_args(req: &RunRequest, prompt: &str, rules: &str) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-p".into(),
        prompt.into(),
        "--output-format".into(),
        "json".into(),
        "--no-auto-update".into(),
        "--disable-web-search".into(),
        "--no-subagents".into(),
        "--disallowed-tools".into(),
        match req.tools {
            ToolAccess::None => "Bash,Edit,Read,Grep,MCPTool,WebFetch,WebSearch".into(),
            ToolAccess::ReadOnly { .. } => "Bash,Edit,MCPTool,WebFetch,WebSearch".into(),
        },
    ];
    if !rules.trim().is_empty() {
        a.push("--rules".into());
        a.push(rules.into());
    }
    if let Some(m) = &req.model {
        a.push("-m".into());
        a.push(m.clone());
    }
    if let Some(e) = &req.effort {
        a.push("--effort".into());
        a.push(e.clone());
    }
    match &req.session {
        SessionMode::New(id) | SessionMode::Resume(id) if !id.is_empty() => {
            a.push("-s".into());
            a.push(id.clone());
        }
        _ => {}
    }
    a
}

fn first_str(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()).filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
    }
    None
}

/// (réponse, session, erreur) depuis la sortie JSON (forme non documentée : lecture souple).
pub(crate) fn parse_output(stdout: &str) -> (String, Option<String>, Option<String>) {
    let whole = serde_json::from_str::<Value>(stdout.trim()).ok();
    let v = whole.or_else(|| stdout.lines().rev().find_map(|l| serde_json::from_str::<Value>(l.trim()).ok()));
    let Some(v) = v else { return (stdout.trim().to_string(), None, None) };
    let answer = first_str(&v, &["result", "response", "text", "content", "output", "message"])
        .or_else(|| v.get("result").and_then(|r| first_str(r, &["text", "content", "response"])))
        .unwrap_or_default();
    let session = first_str(&v, &["session_id", "sessionId", "session"]);
    let error = first_str(&v, &["error"]).or_else(|| v.get("error").and_then(|e| first_str(e, &["message"])));
    (answer, session, error)
}

#[async_trait]
impl AgentAdapter for Grok {
    fn id(&self) -> &'static str {
        "grok"
    }
    fn display_name(&self) -> &'static str {
        "Grok Build"
    }
    fn support(&self) -> Support {
        Support::Experimental
    }
    async fn detect(&self) -> Detection {
        detect_binary(BIN).await
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { resume_session: true, stream_json: false, read_only_files: true, tool_deny_list: false, extra_dirs: false }
    }
    fn efforts(&self) -> Vec<&'static str> {
        vec!["low", "medium", "high", "xhigh"]
    }
    fn lists_models(&self) -> bool {
        true
    }
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(parse_model_list(&run_text(BIN, &["models"]).await?))
    }
    async fn list_mcp(&self) -> Result<Vec<McpServer>> {
        let out = run_text(BIN, &["mcp", "list", "--json"]).await?;
        if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(out.trim()) {
            return Ok(items
                .iter()
                .filter_map(|i| first_str(i, &["name", "id"]))
                .map(|name| McpServer { name, status: "configured".into() })
                .collect());
        }
        Ok(parse_mcp_list(&out))
    }
    async fn sync_instructions(&self, run_dir: &Path, instructions: &Instructions) -> Result<()> {
        std::fs::create_dir_all(run_dir)?;
        std::fs::write(run_dir.join("AGENTS.md"), &instructions.markdown)?;
        Ok(())
    }
    async fn apply_policy(&self, _policy: &ReadOnlyPolicy, _run_dir: &Path) -> Result<()> {
        Ok(())
    }
    async fn run(&self, req: RunRequest, _events: mpsc::Sender<AgentEvent>, cancel: CancellationToken) -> Result<RunResult> {
        if matches!(&req.tools, ToolAccess::ReadOnly { connectors } if !connectors.is_empty()) {
            anyhow::bail!("Grok ne sait pas bloquer les outils MCP en écriture : connecteurs refusés");
        }
        let bin = which::which(BIN).map_err(|_| anyhow::anyhow!("Grok Build (« grok ») introuvable"))?;
        let rules = std::fs::read_to_string(req.run_dir.join("AGENTS.md")).unwrap_or_default();
        let mut prompt = req.prompt.clone();
        if req.tools == ToolAccess::None {
            prompt.push_str("\n\nN'utilise aucun outil : réponds directement.");
        } else if !req.repos.is_empty() {
            prompt.push_str("\n\nDépôts lisibles (lecture seule, outils Read et Grep) :\n");
            for r in &req.repos {
                prompt.push_str(&format!("- {}\n", r.display()));
            }
        }
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(build_args(&req, &prompt, &rules)).current_dir(&req.run_dir);
        strip_env(&mut cmd, req.allow_api_key, API_KEY_ENV);
        let out = run_streaming(cmd, None, req.timeout, cancel, |_| {}).await?;
        if out.cancelled {
            anyhow::bail!("annulée");
        }
        let (answer, session, err) = parse_output(&out.stdout);
        if !out.success() || (answer.is_empty() && err.is_some()) {
            anyhow::bail!("{}", err.unwrap_or_else(|| out.error_message("Grok Build")));
        }
        let session_id = session.or(match &req.session {
            SessionMode::New(id) | SessionMode::Resume(id) if !id.is_empty() => Some(id.clone()),
            _ => None,
        });
        Ok(RunResult { answer, session_id, tools_used: vec![], model: req.model.clone(), denied_tools: vec![] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sortie_et_arguments() {
        assert_eq!(parse_output(r#"{"result":"TL;DR: ok","session_id":"s-1"}"#).0, "TL;DR: ok");
        assert_eq!(parse_output(r#"{"result":"TL;DR: ok","session_id":"s-1"}"#).1.as_deref(), Some("s-1"));
        assert_eq!(parse_output("texte brut").0, "texte brut");
        let r = RunRequest {
            run_dir: PathBuf::from("/tmp"),
            prompt: String::new(),
            tier: Tier::Search,
            model: Some("grok-4.7".into()),
            effort: Some("medium".into()),
            session: SessionMode::New("123e4567-e89b-12d3-a456-426614174000".into()),
            repos: vec![],
            tools: ToolAccess::ReadOnly { connectors: vec![] },
            max_tool_calls: 0,
            timeout: Duration::from_secs(5),
            allow_api_key: false,
            run_id: "x".into(),
            hook_exe: None,
            skills: false,
        };
        let a = build_args(&r, "q", "règles").join(" ");
        assert!(a.contains("-m grok-4.7") && a.contains("--effort medium") && a.contains("-s 123e4567"));
        assert!(a.contains("--disallowed-tools Bash,Edit,MCPTool,WebFetch,WebSearch"));
        assert!(a.contains("--rules règles"));
    }
}
