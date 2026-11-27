//! Adaptateur Antigravity CLI (`agy -p`), expérimental.
//! Doc : https://antigravity.google/docs/cli/reference/ et /docs/cli/headless/.
//!
//! - Non interactif : `agy -p "<invite>" --output-format json` → objet `{conversation_id,
//!   status, response, error, …}`.
//! - Modèle : `--model <slug>` (liste : `agy models`) ; réflexion : `--effort low|…|max`.
//! - Reprise : `--conversation <conversation_id>`.
//! - Lecture seule : en mode non interactif, tout outil qui demande une approbation (écriture,
//!   commande) est refusé d'office (« soft-denied ») ; on ne passe jamais
//!   `--dangerously-skip-permissions`. Les outils MCP ne peuvent pas être filtrés par motif :
//!   connecteurs non activables. À VÉRIFIER (S3).

use super::*;
use crate::agents::process::run_streaming;
use serde_json::Value;

const BIN: &str = "agy";
const API_KEY_ENV: &[&str] = &["GEMINI_API_KEY", "GOOGLE_API_KEY"];

#[derive(Default)]
pub struct Antigravity;

pub(crate) fn build_args(req: &RunRequest, prompt: &str) -> Vec<String> {
    let mut a: Vec<String> = vec!["-p".into(), prompt.into(), "--output-format".into(), "json".into()];
    if let Some(m) = &req.model {
        a.push("--model".into());
        a.push(m.clone());
    }
    if let Some(e) = &req.effort {
        a.push("--effort".into());
        a.push(e.clone());
    }
    if let SessionMode::Resume(id) = &req.session {
        a.push("--conversation".into());
        a.push(id.clone());
    }
    a
}

/// (réponse, identifiant de conversation, erreur).
pub(crate) fn parse_output(stdout: &str) -> (String, Option<String>, Option<String>) {
    // Le JSON est sur une ligne ; d'éventuels avis le précèdent.
    for line in stdout.lines().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
        let obj = v.get("result").filter(|r| r.is_object()).unwrap_or(&v);
        let s = |k: &str| obj.get(k).and_then(|x| x.as_str()).map(String::from);
        let status = s("status").unwrap_or_default();
        let err = s("error").or_else(|| (status != "" && status != "SUCCESS").then(|| format!("statut {status}")));
        return (s("response").unwrap_or_default(), s("conversation_id"), err);
    }
    (stdout.trim().to_string(), None, None)
}

#[async_trait]
impl AgentAdapter for Antigravity {
    fn id(&self) -> &'static str {
        "antigravity"
    }
    fn display_name(&self) -> &'static str {
        "Antigravity"
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
        vec!["low", "medium", "high", "xhigh", "max"]
    }
    fn lists_models(&self) -> bool {
        true
    }
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(parse_model_list(&run_text(BIN, &["models"]).await?))
    }
    async fn list_mcp(&self) -> Result<Vec<McpServer>> {
        // Pas de commande documentée pour lister les serveurs MCP hors mode interactif.
        Ok(vec![])
    }
    async fn sync_instructions(&self, run_dir: &Path, instructions: &Instructions) -> Result<()> {
        std::fs::create_dir_all(run_dir)?;
        // Fichier de consignes lu par Antigravity : À VÉRIFIER ; les consignes sont aussi dans l'invite.
        std::fs::write(run_dir.join("AGENTS.md"), &instructions.markdown)?;
        std::fs::write(run_dir.join("GEMINI.md"), &instructions.markdown)?;
        Ok(())
    }
    async fn apply_policy(&self, _policy: &ReadOnlyPolicy, _run_dir: &Path) -> Result<()> {
        Ok(())
    }
    async fn run(&self, req: RunRequest, _events: mpsc::Sender<AgentEvent>, cancel: CancellationToken) -> Result<RunResult> {
        if matches!(&req.tools, ToolAccess::ReadOnly { connectors } if !connectors.is_empty()) {
            anyhow::bail!("Antigravity ne sait pas bloquer les outils MCP en écriture : connecteurs refusés");
        }
        let bin = which::which(BIN).map_err(|_| anyhow::anyhow!("Antigravity (« agy ») introuvable"))?;
        let instructions = std::fs::read_to_string(req.run_dir.join("AGENTS.md")).unwrap_or_default();
        let mut prompt = format!("{instructions}\n\n---\n\n{}", req.prompt);
        match &req.tools {
            ToolAccess::None => prompt.push_str("\n\nN'utilise aucun outil : réponds directement."),
            ToolAccess::ReadOnly { .. } if !req.repos.is_empty() => {
                prompt.push_str("\n\nDépôts lisibles (lecture seule) :\n");
                for r in &req.repos {
                    prompt.push_str(&format!("- {}\n", r.display()));
                }
            }
            _ => {}
        }
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(build_args(&req, &prompt)).current_dir(&req.run_dir);
        strip_env(&mut cmd, req.allow_api_key, API_KEY_ENV);
        let out = run_streaming(cmd, None, req.timeout, cancel, |_| {}).await?;
        if out.cancelled {
            anyhow::bail!("annulée");
        }
        let (answer, conv, err) = parse_output(&out.stdout);
        if !out.success() || (answer.is_empty() && err.is_some()) {
            anyhow::bail!("{}", err.unwrap_or_else(|| out.error_message("Antigravity")));
        }
        Ok(RunResult { answer, session_id: conv, tools_used: vec![], model: req.model.clone(), denied_tools: vec![] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sortie_json_et_arguments() {
        let (a, c, e) = parse_output(r#"{"conversation_id":"c-1","status":"SUCCESS","response":"TL;DR: ok","num_turns":1}"#);
        assert_eq!((a.as_str(), c.as_deref(), e), ("TL;DR: ok", Some("c-1"), None));
        let (_, _, e) = parse_output(r#"{"conversation_id":"c-1","status":"ERROR","error":"quota"}"#);
        assert_eq!(e.as_deref(), Some("quota"));
        let r = RunRequest {
            run_dir: PathBuf::from("/tmp"),
            prompt: String::new(),
            tier: Tier::Summary,
            model: Some("gemini-x".into()),
            effort: Some("max".into()),
            session: SessionMode::Resume("c-1".into()),
            repos: vec![],
            tools: ToolAccess::None,
            max_tool_calls: 0,
            timeout: Duration::from_secs(5),
            allow_api_key: false,
            run_id: "x".into(),
            hook_exe: None,
            skills: false,
        };
        let a = build_args(&r, "bonjour").join(" ");
        assert!(a.starts_with("-p bonjour --output-format json"));
        assert!(a.contains("--model gemini-x") && a.contains("--effort max") && a.contains("--conversation c-1"));
    }
}
