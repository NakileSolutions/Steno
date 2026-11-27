//! Adaptateur OpenCode (`opencode run`), expérimental.
//!
//! Lecture seule : `opencode.json` dans le dossier d'exécution refuse l'édition, le web et
//! toute commande shell hors de la liste autorisée, et coupe tous les autres outils (dont
//! ceux des serveurs MCP, `"*": false`). Deux agents y sont définis : `steno-lecture`
//! (lecture des dépôts) et `steno-sans-outil` (aucun outil), choisi selon `ToolAccess` par
//! `--agent`. Les outils MCP ne peuvent pas être filtrés par motif : connecteurs non
//! activables. L'invite passe par l'entrée standard (`opencode run` l'ajoute au message),
//! jamais dans la ligne de commande. À VÉRIFIER (S3).

use super::*;
use crate::agents::process::run_streaming;
use serde_json::{json, Value};

const BIN: &str = "opencode";
const API_KEY_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "OPENROUTER_API_KEY",
    "GOOGLE_GENERATIVE_AI_API_KEY",
    "AZURE_API_KEY",
    "MISTRAL_API_KEY",
    "GROQ_API_KEY",
    "DEEPSEEK_API_KEY",
];

#[derive(Default)]
pub struct OpenCode;

const AGENT_READ: &str = "steno-lecture";
const AGENT_NO_TOOLS: &str = "steno-sans-outil";

/// Outils de lecture d'OpenCode autorisés pour une recherche (le shell reste filtré).
const READ_TOOLS: &[&str] = &["read", "grep", "glob", "list", "bash"];

pub(crate) fn config(policy: &ReadOnlyPolicy) -> Value {
    let mut bash = serde_json::Map::new();
    for c in &policy.base.allowed_commands {
        bash.insert(format!("{c} *"), json!("allow"));
        bash.insert(c.clone(), json!("allow"));
    }
    bash.insert("*".into(), json!("deny"));
    // Motifs du plus général au plus précis : `*` coupe tout (outils MCP compris), puis les
    // outils de lecture sont réactivés.
    let mut read = serde_json::Map::new();
    read.insert("*".into(), json!(false));
    for t in ["write", "edit", "patch", "webfetch", "todowrite", "task"] {
        read.insert(t.into(), json!(false));
    }
    for t in READ_TOOLS {
        read.insert((*t).into(), json!(true));
    }
    let mut none = serde_json::Map::new();
    none.insert("*".into(), json!(false));
    for t in READ_TOOLS.iter().chain(&["write", "edit", "patch", "webfetch", "todowrite", "task"]) {
        none.insert((*t).into(), json!(false));
    }
    let read_permission = json!({ "edit": "deny", "webfetch": "deny", "bash": bash });
    let no_permission = json!({ "edit": "deny", "webfetch": "deny", "bash": "deny" });
    json!({
        "$schema": "https://opencode.ai/config.json",
        "permission": read_permission,
        "tools": read,
        "mcp": {},
        "agent": {
            AGENT_READ: {
                "mode": "primary",
                "description": "Sténo : lecture seule des dépôts",
                "tools": read,
                "permission": read_permission,
            },
            AGENT_NO_TOOLS: {
                "mode": "primary",
                "description": "Sténo : aucun outil",
                "tools": none,
                "permission": no_permission,
            },
        }
    })
}

/// Arguments de `opencode run` (l'invite est envoyée à part, sur l'entrée standard).
pub(crate) fn build_args(req: &RunRequest) -> Vec<String> {
    let agent = if req.tools == ToolAccess::None { AGENT_NO_TOOLS } else { AGENT_READ };
    let mut a: Vec<String> = vec!["run".into(), "--format".into(), "json".into(), "--agent".into(), agent.into()];
    if let Some(m) = &req.model {
        a.push("-m".into());
        a.push(m.clone());
    }
    if let SessionMode::Resume(id) = &req.session {
        a.push("--session".into());
        a.push(id.clone());
    }
    a.push("Réponds à la demande transmise sur l'entrée standard, en suivant AGENTS.md.".into());
    a
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
        let mut prompt = req.prompt.clone();
        if req.tools == ToolAccess::None {
            prompt.push_str("\n\nN'utilise aucun outil : réponds directement.");
        } else if !req.repos.is_empty() {
            prompt.push_str("\n\nDépôts lisibles :\n");
            for r in &req.repos {
                prompt.push_str(&format!("- {}\n", r.display()));
            }
        }
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(build_args(&req)).current_dir(&req.run_dir);
        strip_env(&mut cmd, req.allow_api_key, API_KEY_ENV);
        let ev = events.clone();
        let out = run_streaming(cmd, Some(prompt), req.timeout, cancel, |l| {
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
    use super::*;

    fn req(tools: ToolAccess) -> RunRequest {
        RunRequest {
            run_dir: PathBuf::from("/tmp"),
            prompt: "Transcription confidentielle".into(),
            tier: Tier::Quick,
            model: Some("fournisseur/modele".into()),
            effort: None,
            session: SessionMode::Resume("ses_1".into()),
            repos: vec![],
            exclude_globs: vec![],
            tools,
            max_tool_calls: 0,
            timeout: Duration::from_secs(5),
            allow_api_key: false,
            run_id: "x".into(),
            hook_exe: None,
            skills: false,
        }
    }

    #[test]
    fn configuration_selon_les_outils() {
        let mut p = ReadOnlyPolicy::no_tools();
        p.base = crate::policy::PolicyFile::builtin();
        let c = config(&p);
        assert_eq!(c["tools"]["*"], false, "outils MCP et inconnus coupés");
        assert_eq!(c["tools"]["read"], true);
        assert_eq!(c["permission"]["bash"]["*"], "deny");
        assert_eq!(c["permission"]["bash"]["rg *"], "allow");
        let none = &c["agent"][AGENT_NO_TOOLS];
        for t in ["*", "bash", "read", "grep", "glob", "list", "write", "webfetch"] {
            assert_eq!(none["tools"][t], false, "{t} coupé sans outils");
        }
        assert_eq!(none["permission"]["bash"], "deny");
        let a = build_args(&req(ToolAccess::None));
        assert!(a.windows(2).any(|w| w[0] == "--agent" && w[1] == AGENT_NO_TOOLS));
        assert!(!a.join(" ").contains("Transcription"), "l'invite ne passe pas par la ligne de commande");
        let a = build_args(&req(ToolAccess::ReadOnly { connectors: vec![] }));
        assert!(a.windows(2).any(|w| w[0] == "--agent" && w[1] == AGENT_READ));
        assert!(a.join(" ").contains("--session ses_1") && a.join(" ").contains("-m fournisseur/modele"));
    }

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
