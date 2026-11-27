//! Adaptateur Gemini CLI (`gemini -p`), expérimental.
//!
//! Lecture seule : mode d'approbation par défaut (en mode non interactif, les outils qui
//! demandent une approbation — écriture, shell — sont refusés) et aucun serveur MCP
//! autorisé (`--allowed-mcp-server-names` avec un nom inexistant). À VÉRIFIER (S3).

use super::*;
use crate::agents::process::run_streaming;
use serde_json::Value;

const BIN: &str = "gemini";
const API_KEY_ENV: &[&str] = &["GEMINI_API_KEY", "GOOGLE_API_KEY"];

#[derive(Default)]
pub struct Gemini;

pub(crate) fn build_args(req: &RunRequest) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "--output-format".into(),
        "json".into(),
        "--approval-mode".into(),
        "default".into(),
        "--allowed-mcp-server-names".into(),
        "steno-aucun-serveur".into(),
    ];
    if let Some(m) = &req.model {
        a.push("-m".into());
        a.push(m.clone());
    }
    if req.tools != ToolAccess::None {
        for r in &req.repos {
            a.push("--include-directories".into());
            a.push(r.display().to_string());
        }
    }
    a.push("-p".into());
    a.push("Réponds à la demande ci-dessus en suivant GEMINI.md.".into());
    a
}

pub(crate) fn parse_output(stdout: &str) -> (String, Option<String>) {
    if let Ok(v) = serde_json::from_str::<Value>(stdout.trim()) {
        let answer = v.get("response").and_then(|r| r.as_str()).unwrap_or_default().to_string();
        let model = v
            .pointer("/stats/models")
            .and_then(|m| m.as_object())
            .and_then(|m| m.keys().next().cloned());
        return (answer, model);
    }
    (stdout.trim().to_string(), None)
}

#[async_trait]
impl AgentAdapter for Gemini {
    fn id(&self) -> &'static str {
        "gemini"
    }
    fn display_name(&self) -> &'static str {
        "Gemini CLI"
    }
    fn support(&self) -> Support {
        Support::Experimental
    }
    async fn detect(&self) -> Detection {
        detect_binary(BIN).await
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { resume_session: false, stream_json: false, read_only_files: true, tool_deny_list: false, extra_dirs: true }
    }
    async fn list_mcp(&self) -> Result<Vec<McpServer>> {
        run_mcp_list(BIN, &["mcp", "list"]).await
    }
    async fn sync_instructions(&self, run_dir: &Path, instructions: &Instructions) -> Result<()> {
        std::fs::create_dir_all(run_dir)?;
        std::fs::write(run_dir.join("GEMINI.md"), &instructions.markdown)?;
        Ok(())
    }
    async fn apply_policy(&self, _policy: &ReadOnlyPolicy, _run_dir: &Path) -> Result<()> {
        Ok(())
    }
    async fn run(&self, req: RunRequest, _events: mpsc::Sender<AgentEvent>, cancel: CancellationToken) -> Result<RunResult> {
        if matches!(&req.tools, ToolAccess::ReadOnly { connectors } if !connectors.is_empty()) {
            anyhow::bail!("Gemini CLI ne sait pas bloquer les outils MCP en écriture : connecteurs refusés");
        }
        let bin = which::which(BIN).map_err(|_| anyhow::anyhow!("Gemini CLI (« gemini ») introuvable"))?;
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(build_args(&req)).current_dir(&req.run_dir);
        strip_env(&mut cmd, req.allow_api_key, API_KEY_ENV);
        let out = run_streaming(cmd, Some(req.prompt.clone()), req.timeout, cancel, |_| {}).await?;
        if out.cancelled {
            anyhow::bail!("annulée");
        }
        if !out.success() {
            anyhow::bail!("{}", out.error_message("Gemini CLI"));
        }
        let (answer, model) = parse_output(&out.stdout);
        Ok(RunResult { answer, session_id: None, tools_used: vec![], model: model.or(req.model.clone()), denied_tools: vec![] })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sortie_json() {
        let (a, m) = super::parse_output(r#"{"response":"TL;DR: ok","stats":{"models":{"m-1":{}}}}"#);
        assert_eq!(a, "TL;DR: ok");
        assert_eq!(m.as_deref(), Some("m-1"));
        assert_eq!(super::parse_output("texte brut\n").0, "texte brut");
    }
}
