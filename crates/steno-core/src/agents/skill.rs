//! Skill « Sténo » pour l'agent de l'utilisateur, hors de l'app : `agent/skills/steno/SKILL.md`
//! copié dans les dossiers de skills des CLI, et serveur MCP `steno mcp` (lecture seule)
//! déclaré dans leur configuration. Rien n'est installé sans une action de l'utilisateur
//! (`steno skill install` ou bouton des réglages).

use anyhow::{bail, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Contenu du skill (intégré au binaire).
pub const SKILL_MD: &str = include_str!("../../../../agent/skills/steno/SKILL.md");
/// Nom du skill et du serveur MCP.
pub const NAME: &str = "steno";

/// Un dossier de skills et l'état du skill Sténo dedans.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SkillTarget {
    /// Agents qui lisent ce dossier.
    pub agents: String,
    pub path: String,
    /// `missing`, `current` ou `outdated`.
    pub state: String,
}

/// Déclaration du serveur MCP `steno` dans un CLI d'agent.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct McpTarget {
    pub agent: String,
    pub registered: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct SkillStatus {
    pub targets: Vec<SkillTarget>,
    pub mcp: Vec<McpTarget>,
    /// CLI `steno` utilisée par le serveur MCP (`None` : introuvable).
    pub cli: Option<String>,
}

/// Dossiers de skills des agents présents sur le Mac : `~/.claude/skills` (Claude Code) et
/// `~/.agents/skills` (dossier commun lu par Codex et d'autres CLI).
pub fn skill_dirs(home: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut out = vec![];
    if home.join(".claude").is_dir() {
        out.push((
            "Claude Code",
            home.join(".claude").join("skills").join(NAME),
        ));
    }
    if [".agents", ".codex", ".gemini", ".config/opencode"]
        .iter()
        .any(|d| home.join(d).is_dir())
    {
        out.push((
            "Codex, Gemini CLI, OpenCode…",
            home.join(".agents").join("skills").join(NAME),
        ));
    }
    out
}

fn target_state(dir: &Path) -> &'static str {
    match std::fs::read_to_string(dir.join("SKILL.md")) {
        Ok(s) if s == SKILL_MD => "current",
        Ok(_) => "outdated",
        Err(_) => "missing",
    }
}

/// CLI `steno` installée par `make install` (`~/.local/bin/steno`), sinon celle du PATH.
pub fn steno_cli(home: &Path) -> Option<PathBuf> {
    let local = home.join(".local").join("bin").join("steno");
    if local.is_file() {
        return Some(local);
    }
    which::which("steno").ok()
}

/// CLI d'agent qui savent déclarer un serveur MCP : (nom affiché, binaire, arguments d'ajout).
fn mcp_clis(cli: &Path) -> Vec<(&'static str, &'static str, Vec<String>)> {
    let exe = cli.display().to_string();
    vec![
        (
            "Claude Code",
            "claude",
            vec![
                "mcp".into(),
                "add".into(),
                "--scope".into(),
                "user".into(),
                NAME.into(),
                "--".into(),
                exe.clone(),
                "mcp".into(),
            ],
        ),
        (
            "Codex",
            "codex",
            vec![
                "mcp".into(),
                "add".into(),
                NAME.into(),
                "--".into(),
                exe,
                "mcp".into(),
            ],
        ),
    ]
}

async fn run_ok(bin: &Path, args: &[&str]) -> bool {
    let out = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(bin)
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await;
    matches!(out, Ok(Ok(o)) if o.status.success())
}

/// État du skill et du serveur MCP.
pub async fn status() -> SkillStatus {
    let home = dirs::home_dir().unwrap_or_default();
    let cli = steno_cli(&home);
    let targets = skill_dirs(&home)
        .into_iter()
        .map(|(agents, dir)| SkillTarget {
            agents: agents.into(),
            state: target_state(&dir).into(),
            path: dir.display().to_string(),
        })
        .collect();
    let mut mcp = vec![];
    for (agent, bin, _) in mcp_clis(cli.as_deref().unwrap_or(Path::new("steno"))) {
        if let Ok(path) = which::which(bin) {
            mcp.push(McpTarget {
                agent: agent.into(),
                registered: run_ok(&path, &["mcp", "get", NAME]).await,
            });
        }
    }
    SkillStatus {
        targets,
        mcp,
        cli: cli.map(|c| c.display().to_string()),
    }
}

/// Copie le skill dans les dossiers des agents présents et déclare le serveur MCP là où il
/// manque. Relancer met le skill à jour.
pub async fn install() -> Result<SkillStatus> {
    let home = dirs::home_dir().unwrap_or_default();
    let Some(cli) = steno_cli(&home) else {
        bail!("CLI « steno » introuvable : installez Sténo avec « make install »");
    };
    let dirs = skill_dirs(&home);
    let clis: Vec<_> = mcp_clis(&cli)
        .into_iter()
        .filter_map(|(a, b, args)| which::which(b).ok().map(|p| (a, p, args)))
        .collect();
    if dirs.is_empty() && clis.is_empty() {
        bail!("aucun agent compatible trouvé (Claude Code, Codex…)");
    }
    for (_, dir) in &dirs {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("SKILL.md"), SKILL_MD)?;
    }
    for (agent, path, args) in clis {
        if run_ok(&path, &["mcp", "get", NAME]).await {
            continue;
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        if !run_ok(&path, &args).await {
            tracing::warn!("{agent} : déclaration du serveur MCP {NAME} impossible");
        }
    }
    Ok(status().await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_valide() {
        let fm = SKILL_MD
            .strip_prefix("---\n")
            .and_then(|s| s.split_once("\n---\n"))
            .map(|(f, _)| f)
            .unwrap();
        assert!(fm.lines().any(|l| l == "name: steno"));
        assert!(fm
            .lines()
            .any(|l| l.starts_with("description: ") && l.len() < 1100));
        // Chaque outil cité existe dans le serveur MCP.
        for tool in [
            "search_meetings",
            "get_meeting",
            "get_report",
            "get_transcript",
            "list_decisions",
            "list_actions",
            "list_report_templates",
        ] {
            assert!(SKILL_MD.contains(tool), "{tool}");
        }
    }

    #[test]
    fn dossiers_et_etat() {
        let d = tempfile::tempdir().unwrap();
        assert!(skill_dirs(d.path()).is_empty());
        std::fs::create_dir_all(d.path().join(".claude")).unwrap();
        std::fs::create_dir_all(d.path().join(".codex")).unwrap();
        let dirs = skill_dirs(d.path());
        assert_eq!(dirs.len(), 2);
        assert!(
            dirs[0].1.ends_with(".claude/skills/steno")
                && dirs[1].1.ends_with(".agents/skills/steno")
        );
        let dir = &dirs[0].1;
        assert_eq!(target_state(dir), "missing");
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "ancien").unwrap();
        assert_eq!(target_state(dir), "outdated");
        std::fs::write(dir.join("SKILL.md"), SKILL_MD).unwrap();
        assert_eq!(target_state(dir), "current");
    }
}
