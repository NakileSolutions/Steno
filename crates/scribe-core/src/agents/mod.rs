//! Adaptateurs d'agent IA. **Seul endroit** du code qui connaît les noms des CLI, leurs
//! options et leurs formats de sortie (PLAN §5.3, docs/contracts.md §3).

mod antigravity;
mod claude_code;
mod codex;
mod custom;
mod gemini;
mod grok;
mod opencode;
pub mod process;

use crate::model::Tier;
use crate::policy::ReadOnlyPolicy;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub use antigravity::Antigravity;
pub use claude_code::ClaudeCode;
pub use grok::Grok;
pub use codex::Codex;
pub use custom::Custom;
pub use gemini::Gemini;
pub use opencode::OpenCode;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Support {
    Supported,
    Experimental,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Detection {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Capabilities {
    pub resume_session: bool,
    pub stream_json: bool,
    pub read_only_files: bool,
    pub tool_deny_list: bool,
    pub extra_dirs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub status: String,
}

/// Modèle proposé par un CLI (liste indicative : la saisie libre reste possible).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub note: Option<String>,
}

impl ModelInfo {
    pub fn new(id: &str, name: &str, note: Option<&str>) -> ModelInfo {
        ModelInfo { id: id.into(), name: name.into(), note: note.map(String::from) }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub support: Support,
    pub installed: bool,
    pub version: Option<String>,
    pub error: Option<String>,
    pub capabilities: Capabilities,
    /// Niveaux de réflexion acceptés (vide = non réglable).
    pub efforts: Vec<String>,
    /// `list_models` interroge réellement le CLI.
    pub lists_models: bool,
}

/// Consignes à installer dans le dossier d'exécution.
#[derive(Debug, Clone)]
pub struct Instructions {
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionMode {
    None,
    New(String),
    Resume(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolAccess {
    None,
    ReadOnly { connectors: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct RunRequest {
    pub run_dir: PathBuf,
    pub prompt: String,
    pub tier: Tier,
    pub model: Option<String>,
    /// Niveau de réflexion (« low », « high »…), `None` = défaut du CLI.
    pub effort: Option<String>,
    pub session: SessionMode,
    pub repos: Vec<PathBuf>,
    pub tools: ToolAccess,
    pub max_tool_calls: u32,
    pub timeout: Duration,
    /// Autoriser les clés d'API présentes dans l'environnement (désactivé par défaut).
    pub allow_api_key: bool,
    /// Identifiant unique de l'exécution (fichiers temporaires).
    pub run_id: String,
    /// Exécutable qui sait répondre à `policy-hook <fichier>`.
    pub hook_exe: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Started { session_id: Option<String> },
    ToolCall { name: String },
    Text(String),
    Stderr(String),
}

#[derive(Debug, Clone, Default)]
pub struct RunResult {
    pub answer: String,
    pub session_id: Option<String>,
    pub tools_used: Vec<String>,
    pub model: Option<String>,
    pub denied_tools: Vec<String>,
}

#[async_trait]
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn support(&self) -> Support;
    async fn detect(&self) -> Detection;
    fn capabilities(&self) -> Capabilities;
    /// Niveaux de réflexion acceptés par le CLI (vide = non réglable).
    fn efforts(&self) -> Vec<&'static str> {
        vec![]
    }
    /// `true` si `list_models` interroge le CLI (sinon : suggestions statiques).
    fn lists_models(&self) -> bool {
        false
    }
    /// Modèles proposés (indicatif).
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![])
    }
    async fn list_mcp(&self) -> Result<Vec<McpServer>>;
    async fn sync_instructions(&self, run_dir: &Path, instructions: &Instructions) -> Result<()>;
    async fn apply_policy(&self, policy: &ReadOnlyPolicy, run_dir: &Path) -> Result<()>;
    async fn run(&self, req: RunRequest, events: mpsc::Sender<AgentEvent>, cancel: CancellationToken) -> Result<RunResult>;
}

/// Construit l'adaptateur choisi. `custom_command` sert à l'adaptateur `custom`.
pub fn adapter_for(id: &str, custom_command: &str) -> Option<Arc<dyn AgentAdapter>> {
    Some(match id {
        "claude-code" => Arc::new(ClaudeCode),
        "codex" => Arc::new(Codex),
        "antigravity" => Arc::new(Antigravity),
        "grok" => Arc::new(Grok),
        "gemini" => Arc::new(Gemini),
        "opencode" => Arc::new(OpenCode),
        "custom" => Arc::new(Custom::new(custom_command)),
        _ => return None,
    })
}

pub const ALL_IDS: [&str; 7] = ["claude-code", "codex", "antigravity", "grok", "gemini", "opencode", "custom"];

/// Détecte tous les agents connus.
pub async fn detect_all(custom_command: &str) -> Vec<AgentInfo> {
    let mut out = vec![];
    for id in ALL_IDS {
        let a = adapter_for(id, custom_command).expect("adaptateur connu");
        let d = a.detect().await;
        out.push(AgentInfo {
            id: a.id().into(),
            name: a.display_name().into(),
            support: a.support(),
            installed: d.installed,
            version: d.version,
            error: d.error,
            capabilities: a.capabilities(),
            efforts: a.efforts().into_iter().map(String::from).collect(),
            lists_models: a.lists_models(),
        });
    }
    out
}

/// Prépare le dossier d'exécution : dossier, consignes, politique.
pub async fn prepare_run_dir(
    adapter: &dyn AgentAdapter,
    run_dir: &Path,
    instructions: &Instructions,
    policy: &ReadOnlyPolicy,
) -> Result<()> {
    std::fs::create_dir_all(run_dir.join(".scribe"))?;
    adapter.sync_instructions(run_dir, instructions).await?;
    adapter.apply_policy(policy, run_dir).await?;
    Ok(())
}

/// Chemin du fichier de politique d'une exécution.
pub fn policy_file(run_dir: &Path, run_id: &str) -> PathBuf {
    run_dir.join(".scribe").join(format!("policy-{run_id}.json"))
}

/// Construit la politique d'une exécution et l'écrit sur disque.
pub fn write_run_policy(req: &RunRequest) -> Result<ReadOnlyPolicy> {
    let mut readable = vec![req.run_dir.clone()];
    readable.extend(req.repos.iter().cloned());
    let (tools_enabled, servers) = match &req.tools {
        ToolAccess::None => (false, vec![]),
        ToolAccess::ReadOnly { connectors } => (true, connectors.clone()),
    };
    std::fs::create_dir_all(req.run_dir.join(".scribe"))?;
    let counter = req.run_dir.join(".scribe").join(format!("count-{}", req.run_id));
    let _ = std::fs::remove_file(&counter);
    let p = ReadOnlyPolicy {
        base: crate::policy::PolicyFile::load(),
        readable_dirs: readable,
        tools_enabled,
        allowed_mcp_servers: servers,
        max_tool_calls: req.max_tool_calls,
        counter_file: Some(counter),
    };
    p.write_to(&policy_file(&req.run_dir, &req.run_id))?;
    Ok(p)
}

/// Supprime les fichiers temporaires d'une exécution.
pub fn cleanup_run(req: &RunRequest) {
    let d = req.run_dir.join(".scribe");
    let _ = std::fs::remove_file(policy_file(&req.run_dir, &req.run_id));
    let _ = std::fs::remove_file(d.join(format!("count-{}", req.run_id)));
    let _ = std::fs::remove_file(d.join(format!("settings-{}.json", req.run_id)));
    let _ = std::fs::remove_file(d.join(format!("prompt-{}.md", req.run_id)));
}

/// Analyse générique de `<cli> mcp list` : lignes « nom: cible - statut » ou tableau.
pub fn parse_mcp_list(out: &str) -> Vec<McpServer> {
    let re = regex::Regex::new(r"^\s*(?:[✓✗⚠!•*-]\s+)?(?P<name>[^:]+?):\s+(?P<target>.*?)\s+-\s+(?P<status>.+?)\s*$").unwrap();
    let mut v = vec![];
    for line in out.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with("Checking") || l.starts_with("No MCP") || l.starts_with("Configured") {
            continue;
        }
        if let Some(c) = re.captures(l) {
            let status_raw = c["status"].to_string();
            let s = status_raw.to_lowercase();
            let status = if s.contains("connected") && !s.contains("fail") && !s.contains("not") {
                "connected"
            } else if s.contains("auth") {
                "needs_auth"
            } else if s.contains("pending") {
                "pending"
            } else if s.contains("fail") || s.contains("error") || s.contains("✗") {
                "failed"
            } else {
                "unknown"
            };
            v.push(McpServer { name: c["name"].trim().to_string(), status: status.into() });
        }
    }
    if v.is_empty() {
        // Tableau : première colonne, après une ligne d'en-tête « Name ».
        let mut in_table = false;
        for line in out.lines() {
            let l = line.trim();
            if l.to_lowercase().starts_with("name") {
                in_table = true;
                continue;
            }
            if in_table && !l.is_empty() && !l.starts_with('-') && !l.starts_with('─') {
                if let Some(name) = l.split_whitespace().next() {
                    v.push(McpServer { name: name.to_string(), status: "configured".into() });
                }
            }
        }
    }
    v
}

/// Supprime des variables d'environnement (clés d'API) sauf autorisation explicite.
pub(crate) fn strip_env(cmd: &mut tokio::process::Command, allow_api_key: bool, keys: &[&str]) {
    if !allow_api_key {
        for k in keys {
            cmd.env_remove(k);
        }
    }
}

/// Version d'un exécutable (`<bin> --version`), avec délai.
pub(crate) async fn detect_binary(bin: &str) -> Detection {
    let Ok(path) = which::which(bin) else {
        return Detection { installed: false, error: Some(format!("« {bin} » introuvable dans le PATH")), ..Default::default() };
    };
    let out = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(&path).arg("--version").kill_on_drop(true).output(),
    )
    .await;
    match out {
        Ok(Ok(o)) => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let s = if s.is_empty() { String::from_utf8_lossy(&o.stderr).trim().to_string() } else { s };
            Detection {
                installed: true,
                version: s.lines().next().map(|l| l.to_string()),
                path: Some(path.display().to_string()),
                error: None,
            }
        }
        Ok(Err(e)) => Detection { installed: false, path: Some(path.display().to_string()), error: Some(e.to_string()), ..Default::default() },
        Err(_) => Detection {
            installed: true,
            path: Some(path.display().to_string()),
            error: Some("« --version » ne répond pas".into()),
            ..Default::default()
        },
    }
}

/// Analyse générique d'une liste de modèles affichée par un CLI : une ligne par modèle,
/// identifiant en premier mot, nom lisible ensuite (en-têtes et lignes décoratives ignorés).
pub fn parse_model_list(out: &str) -> Vec<ModelInfo> {
    let mut v: Vec<ModelInfo> = vec![];
    for line in out.lines() {
        let l = line.trim().trim_start_matches(['*', '•', '-', '>', ' ']).trim();
        if l.is_empty() {
            continue;
        }
        let mut parts = l.splitn(2, char::is_whitespace);
        let id = parts.next().unwrap_or("").trim_matches(|c| c == '`' || c == ',' || c == ':');
        let rest = parts.next().unwrap_or("").trim().trim_start_matches(['-', '–', ':', '|']).trim();
        let looks_like_id = id.len() >= 3
            && id.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:@".contains(c))
            && id.chars().any(|c| c.is_ascii_digit() || c == '-')
            && !id.eq_ignore_ascii_case("model")
            && !id.eq_ignore_ascii_case("models");
        if !looks_like_id || v.iter().any(|m| m.id == id) {
            continue;
        }
        let name = if rest.is_empty() { id.to_string() } else { rest.split('|').next().unwrap_or(rest).trim().to_string() };
        v.push(ModelInfo { id: id.to_string(), name, note: None });
    }
    v
}

/// Lance `<bin> <args>` et renvoie la sortie standard (délai 60 s).
pub(crate) async fn run_text(bin: &str, args: &[&str]) -> Result<String> {
    let path = which::which(bin).map_err(|_| anyhow::anyhow!("« {bin} » introuvable"))?;
    let o = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::process::Command::new(path).args(args).kill_on_drop(true).output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("« {bin} {} » ne répond pas", args.join(" ")))??;
    if !o.status.success() {
        anyhow::bail!("« {bin} {} » a échoué : {}", args.join(" "), String::from_utf8_lossy(&o.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&o.stdout).to_string())
}

/// Lance `<bin> mcp list` et analyse la sortie.
pub(crate) async fn run_mcp_list(bin: &str, args: &[&str]) -> Result<Vec<McpServer>> {
    let path = which::which(bin).map_err(|_| anyhow::anyhow!("« {bin} » introuvable"))?;
    let o = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new(path).args(args).kill_on_drop(true).output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("la liste des connecteurs ne répond pas (90 s)"))??;
    let mut text = String::from_utf8_lossy(&o.stdout).to_string();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&o.stderr));
    Ok(parse_mcp_list(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liste_de_modeles() {
        let out = "Available models\n\ngemini-4-neon-xhigh            Gemini 4 Neon (XHigh)\ngemini-3.8-flash-medium   Gemini 3.8 Flash\n";
        let v = parse_model_list(out);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "gemini-4-neon-xhigh");
        assert_eq!(v[0].name, "Gemini 4 Neon (XHigh)");
        let v = parse_model_list("MODEL       CONTEXT\ngrok-4.7    2M\ngrok-4.5    256k\n");
        assert_eq!(v.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["grok-4.7", "grok-4.5"]);
    }

    #[test]
    fn liste_mcp() {
        let out = "Checking MCP server health…\n\nclaude.ai Atlassian: https://mcp.atlassian.com/v1/sse - ✓ Connected\ndatadog: npx datadog-mcp - ✗ Failed to connect\nslack: https://x - ! Needs authentication\n";
        let v = parse_mcp_list(out);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].name, "claude.ai Atlassian");
        assert_eq!(v[0].status, "connected");
        assert_eq!(v[1].status, "failed");
        assert_eq!(v[2].status, "needs_auth");
        let table = "Name      Command   Args\natlassian npx       -y mcp\ngitlab    glab      mcp\n";
        let v = parse_mcp_list(table);
        assert_eq!(v.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["atlassian", "gitlab"]);
    }
}
