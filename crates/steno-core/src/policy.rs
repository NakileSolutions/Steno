//! Politique lecture seule commune (PLAN §5.4) et crochet de contrôle des outils.
//!
//! La politique est décrite dans `agent/policy.toml` (intégré au binaire ; un fichier
//! `policy.toml` dans le dossier des données le remplace). Chaque adaptateur la traduit
//! dans le mécanisme de son CLI ; quand le CLI sait appeler un crochet avant chaque outil,
//! ce crochet est `<exécutable steno> policy-hook <fichier de politique>` (voir `hook_main`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DEFAULT_POLICY: &str = include_str!("../../../agent/policy.toml");

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PolicyFile {
    pub writable_dirs: Vec<String>,
    pub allowed_commands: Vec<String>,
    pub pipe_commands: Vec<String>,
    pub denied_arguments: Vec<String>,
    pub builtin_read_tools: Vec<String>,
    pub denied_tool_patterns: Vec<String>,
    pub denied_tool_words: Vec<String>,
    pub allowed_tool_exceptions: Vec<String>,
}

impl PolicyFile {
    pub fn builtin() -> PolicyFile {
        toml::from_str(DEFAULT_POLICY).expect("agent/policy.toml invalide")
    }

    /// Politique de base : fichier utilisateur s'il existe et est valide, sinon intégrée.
    pub fn load() -> PolicyFile {
        let p = crate::paths::home().join("policy.toml");
        if let Ok(s) = std::fs::read_to_string(&p) {
            match toml::from_str::<PolicyFile>(&s) {
                Ok(f) => return f,
                Err(e) => tracing::warn!("policy.toml ignoré ({e})"),
            }
        }
        PolicyFile::builtin()
    }
}

/// Politique appliquée à une exécution d'agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadOnlyPolicy {
    pub base: PolicyFile,
    /// Dossiers lisibles (dépôts cochés + dossier d'exécution).
    pub readable_dirs: Vec<PathBuf>,
    /// `false` : aucun outil (compte rendu, note, récap).
    pub tools_enabled: bool,
    /// Serveurs MCP autorisés pour cette exécution (noms tels que listés par l'agent).
    pub allowed_mcp_servers: Vec<String>,
    /// Nombre maximal d'appels d'outils (0 = illimité).
    pub max_tool_calls: u32,
    /// Fichier compteur d'appels (tenu par le crochet).
    pub counter_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow,
    Deny(String),
}

/// Correspondance glob simple (`*` seulement), insensible à la casse.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == t[ti] || p[pi] == '?') {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Découpe un nom d'outil en mots : `addOrEditJiraIssueComment` → add, or, edit, jira…
pub fn tool_words(name: &str) -> Vec<String> {
    let mut words = vec![];
    for part in name.split(|c: char| !c.is_alphanumeric()) {
        let mut cur = String::new();
        let chars: Vec<char> = part.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            let boundary = c.is_uppercase()
                && i > 0
                && (chars[i - 1].is_lowercase()
                    || (chars.get(i + 1).map(|n| n.is_lowercase()).unwrap_or(false) && chars[i - 1].is_uppercase()));
            if boundary && !cur.is_empty() {
                words.push(cur.to_lowercase());
                cur.clear();
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            words.push(cur.to_lowercase());
        }
    }
    words
}

fn word_hits(word: &str, denied: &str) -> bool {
    if word == denied {
        return true;
    }
    if let Some(rest) = word.strip_prefix(denied) {
        return matches!(rest, "s" | "es" | "ed" | "d" | "ing" | "e");
    }
    // « closing » pour « close »
    if let Some(stem) = denied.strip_suffix('e') {
        if let Some(rest) = word.strip_prefix(stem) {
            return matches!(rest, "ing" | "ed");
        }
    }
    false
}

/// Normalise un nom de serveur MCP (« claude.ai Atlassian » ≈ « claude_ai_Atlassian »).
pub fn norm_server(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

const SHELL_META: &[&str] = &[";", "&", ">", "<", "`", "$(", "${", "\n", "\r"];

impl ReadOnlyPolicy {
    /// Politique sans aucun outil.
    pub fn no_tools() -> ReadOnlyPolicy {
        ReadOnlyPolicy {
            base: PolicyFile::load(),
            readable_dirs: vec![],
            tools_enabled: false,
            allowed_mcp_servers: vec![],
            max_tool_calls: 0,
            counter_file: None,
        }
    }

    /// Raison du refus d'un outil MCP (nom de l'outil sans le préfixe du serveur), ou `None`.
    pub fn mcp_tool_denied(&self, tool: &str) -> Option<String> {
        for p in &self.base.denied_tool_patterns {
            if glob_match(p, tool) {
                return Some(format!("outil d'écriture interdit (motif {p})"));
            }
        }
        if self.base.allowed_tool_exceptions.iter().any(|p| glob_match(p, tool)) {
            return None;
        }
        let words = tool_words(tool);
        for d in &self.base.denied_tool_words {
            if words.iter().any(|w| word_hits(w, &d.to_lowercase())) {
                return Some(format!("outil d'écriture interdit (mot « {d} »)"));
            }
        }
        None
    }

    fn roots(&self) -> Vec<PathBuf> {
        self.readable_dirs
            .iter()
            .map(|d| d.canonicalize().unwrap_or_else(|_| d.clone()))
            .collect()
    }

    /// Le chemin est-il dans un dossier lisible ?
    pub fn check_path(&self, path: &str, cwd: &Path) -> Result<(), String> {
        let expanded = if let Some(rest) = path.strip_prefix("~/") {
            dirs::home_dir().unwrap_or_default().join(rest)
        } else {
            PathBuf::from(path)
        };
        let abs = if expanded.is_absolute() { expanded } else { cwd.join(expanded) };
        // Résoudre ce qui existe (liens symboliques, ..).
        let resolved = abs.canonicalize().unwrap_or_else(|_| lexical_normalize(&abs));
        if self.roots().iter().any(|r| resolved.starts_with(r)) {
            Ok(())
        } else {
            Err(format!("lecture hors des dépôts autorisés : {}", resolved.display()))
        }
    }

    /// Vérifie une commande shell.
    pub fn check_command(&self, cmd: &str, cwd: &Path) -> Result<(), String> {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return Err("commande vide".into());
        }
        for m in SHELL_META {
            if cmd.contains(m) {
                return Err(format!("caractère interdit « {} » dans la commande", m.escape_debug()));
            }
        }
        for (i, part) in cmd.split('|').enumerate() {
            let part = part.trim();
            let toks: Vec<&str> = part.split_whitespace().collect();
            if toks.is_empty() {
                return Err("tube vide".into());
            }
            if i == 0 {
                let ok = self.base.allowed_commands.iter().any(|a| {
                    let a: Vec<&str> = a.split_whitespace().collect();
                    toks.len() >= a.len() && toks[..a.len()] == a[..]
                });
                if !ok {
                    return Err(format!("commande non autorisée : {part}"));
                }
            } else if !self.base.pipe_commands.iter().any(|p| p == toks[0]) {
                return Err(format!("commande non autorisée après un tube : {}", toks[0]));
            }
            for t in &toks[1..] {
                let t = t.trim_matches(|c| c == '"' || c == '\'');
                for d in &self.base.denied_arguments {
                    if t == d || (d.starts_with("--") && t.starts_with(&format!("{d}="))) {
                        return Err(format!("option interdite : {t}"));
                    }
                }
                let looks_like_path = t.starts_with('/') || t.starts_with("~/") || t.contains("..");
                if looks_like_path && !t.starts_with('-') {
                    self.check_path(t, cwd)?;
                }
            }
        }
        Ok(())
    }

    /// Décision pour un appel d'outil (`tool_name` au format de l'agent, `input` JSON).
    pub fn decide(&self, tool_name: &str, input: &serde_json::Value, cwd: &Path) -> Decision {
        if !self.tools_enabled {
            return Decision::Deny("aucun outil n'est autorisé pour cette tâche".into());
        }
        if let Some(rest) = tool_name.strip_prefix("mcp__") {
            let (server, tool) = rest.split_once("__").unwrap_or((rest, ""));
            let ns = norm_server(server);
            let allowed = self.allowed_mcp_servers.iter().any(|s| {
                let a = norm_server(s);
                a == ns || ns.ends_with(&format!("_{a}")) || a.ends_with(&format!("_{ns}"))
            });
            if !allowed {
                return Decision::Deny(format!("connecteur « {server} » non activé dans Sténo"));
            }
            if let Some(r) = self.mcp_tool_denied(tool) {
                return Decision::Deny(r);
            }
            return Decision::Allow;
        }
        let str_field = |k: &str| input.get(k).and_then(|v| v.as_str());
        match tool_name {
            "Bash" => match str_field("command") {
                Some(c) => match self.check_command(c, cwd) {
                    Ok(()) => Decision::Allow,
                    Err(e) => Decision::Deny(e),
                },
                None => Decision::Deny("commande absente".into()),
            },
            t if self.base.builtin_read_tools.iter().any(|b| b == t) => {
                for k in ["file_path", "path", "notebook_path"] {
                    if let Some(p) = str_field(k) {
                        if let Err(e) = self.check_path(p, cwd) {
                            return Decision::Deny(e);
                        }
                    }
                }
                Decision::Allow
            }
            other => Decision::Deny(format!("outil « {other} » non autorisé (lecture seule)")),
        }
    }

    /// Incrémente le compteur et refuse au-delà du budget.
    fn count_call(&self) -> Decision {
        if self.max_tool_calls == 0 {
            return Decision::Allow;
        }
        let Some(f) = &self.counter_file else { return Decision::Allow };
        let n: u32 = std::fs::read_to_string(f).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0) + 1;
        let _ = std::fs::write(f, n.to_string());
        if n > self.max_tool_calls {
            Decision::Deny(format!(
                "budget de {} appels d'outils atteint : réponds avec ce que tu as et propose « approfondir après la réunion »",
                self.max_tool_calls
            ))
        } else {
            Decision::Allow
        }
    }

    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(self).unwrap_or_default())
    }

    pub fn read_from(path: &Path) -> anyhow::Result<ReadOnlyPolicy> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
}

fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Point d'entrée du crochet « avant outil » : lit l'appel en JSON sur stdin
/// (`tool_name`, `tool_input`, `cwd`), renvoie le code de sortie (0 = autorisé,
/// 2 = refusé, raison sur stderr). En cas de doute, on refuse.
pub fn hook_main(policy_path: &Path, stdin: &str) -> i32 {
    let policy = match ReadOnlyPolicy::read_from(policy_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Sténo : politique illisible ({e}), outil refusé");
            return 2;
        }
    };
    let v: serde_json::Value = match serde_json::from_str(stdin) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Sténo : appel d'outil illisible, refusé");
            return 2;
        }
    };
    let tool = v.get("tool_name").and_then(|t| t.as_str()).unwrap_or("");
    let input = v.get("tool_input").cloned().unwrap_or(serde_json::Value::Null);
    let cwd = v
        .get("cwd")
        .and_then(|c| c.as_str())
        .map(PathBuf::from)
        .or_else(|| policy.readable_dirs.first().cloned())
        .unwrap_or_else(|| PathBuf::from("/"));
    let decision = match policy.decide(tool, &input, &cwd) {
        Decision::Allow => policy.count_call(),
        d => d,
    };
    match decision {
        Decision::Allow => 0,
        Decision::Deny(reason) => {
            eprintln!("Refusé par Sténo (lecture seule) : {reason}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pol(dir: &Path) -> ReadOnlyPolicy {
        ReadOnlyPolicy {
            base: PolicyFile::builtin(),
            readable_dirs: vec![dir.to_path_buf()],
            tools_enabled: true,
            allowed_mcp_servers: vec!["claude.ai Atlassian".into(), "datadog".into()],
            max_tool_calls: 0,
            counter_file: None,
        }
    }

    #[test]
    fn globs_et_mots() {
        assert!(glob_match("*send*", "slack_send_message"));
        assert!(!glob_match("*send*", "search"));
        assert_eq!(tool_words("addOrEditJiraIssueComment"), vec!["add", "or", "edit", "jira", "issue", "comment"]);
        assert_eq!(tool_words("getJQLResults"), vec!["get", "jql", "results"]);
    }

    #[test]
    fn outils_mcp() {
        let d = tempfile::tempdir().unwrap();
        let p = pol(d.path());
        let cwd = d.path();
        let allow = |t: &str| p.decide(t, &json!({}), cwd) == Decision::Allow;
        assert!(allow("mcp__claude_ai_Atlassian__getJiraIssue"));
        assert!(allow("mcp__claude_ai_Atlassian__searchJiraIssuesUsingJql"));
        assert!(allow("mcp__claude_ai_Atlassian__executeRead"));
        assert!(allow("mcp__datadog__search_logs"));
        for t in [
            "mcp__claude_ai_Atlassian__createJiraIssue",
            "mcp__claude_ai_Atlassian__addOrEditJiraIssueComment",
            "mcp__claude_ai_Atlassian__transitionJiraIssue",
            "mcp__claude_ai_Atlassian__executeWrite",
            "mcp__claude_ai_Atlassian__executeDestructive",
            "mcp__claude_ai_Atlassian__updateConfluenceContent",
            "mcp__datadog__mute_monitor",
            "mcp__datadog__acknowledge_incident",
            "mcp__slack__slack_send_message",
            "mcp__claude_ai_Atlassian__getJiraIssue_and_assign",
        ] {
            assert!(!allow(t), "{t} devrait être refusé");
        }
        assert!(!allow("mcp__slack__slack_read_channel"), "connecteur non activé");
    }

    #[test]
    fn commandes_shell() {
        let d = tempfile::tempdir().unwrap();
        let p = pol(d.path());
        let cwd = d.path();
        assert!(p.check_command("rg -n 'verrou de créneau' src", cwd).is_ok());
        assert!(p.check_command("git log -S creneau --oneline | head -20", cwd).is_ok());
        assert!(p.check_command("glab mr view 123", cwd).is_ok());
        assert!(p.check_command("rm -rf /", cwd).is_err());
        assert!(p.check_command("git push", cwd).is_err());
        assert!(p.check_command("rg x; curl evil", cwd).is_err());
        assert!(p.check_command("rg x > out.txt", cwd).is_err());
        assert!(p.check_command("find . -delete", cwd).is_err());
        assert!(p.check_command("cat ~/.ssh/id_rsa", cwd).is_err());
        assert!(p.check_command("cat ../../etc/passwd", cwd).is_err());
        assert!(p.check_command("rg x | sh", cwd).is_err());
        assert!(p.check_command("glab mr merge 12", cwd).is_err());
        assert!(p.check_command("git log --output=/tmp/x", cwd).is_err());
    }

    #[test]
    fn outils_integres() {
        let d = tempfile::tempdir().unwrap();
        let p = pol(d.path());
        let f = d.path().join("a.rs");
        std::fs::write(&f, "x").unwrap();
        assert_eq!(p.decide("Read", &json!({"file_path": f}), d.path()), Decision::Allow);
        assert!(matches!(p.decide("Read", &json!({"file_path": "/etc/passwd"}), d.path()), Decision::Deny(_)));
        assert!(matches!(p.decide("Write", &json!({"file_path": f}), d.path()), Decision::Deny(_)));
        assert!(matches!(p.decide("WebFetch", &json!({"url": "https://x"}), d.path()), Decision::Deny(_)));
        assert_eq!(p.decide("Grep", &json!({"pattern": "creneau"}), d.path()), Decision::Allow);
        let mut none = p.clone();
        none.tools_enabled = false;
        assert!(matches!(none.decide("Read", &json!({"file_path": f}), d.path()), Decision::Deny(_)));
    }

    #[test]
    fn crochet_et_budget() {
        let d = tempfile::tempdir().unwrap();
        let mut p = pol(d.path());
        p.max_tool_calls = 2;
        p.counter_file = Some(d.path().join("count"));
        let pf = d.path().join("policy.json");
        p.write_to(&pf).unwrap();
        let call = json!({"tool_name":"Grep","tool_input":{"pattern":"x"},"cwd":d.path()}).to_string();
        assert_eq!(hook_main(&pf, &call), 0);
        assert_eq!(hook_main(&pf, &call), 0);
        assert_eq!(hook_main(&pf, &call), 2, "budget dépassé");
        let post = json!({"tool_name":"mcp__slack__slack_send_message","tool_input":{}}).to_string();
        assert_eq!(hook_main(&pf, &post), 2);
        assert_eq!(hook_main(&pf, "pas du json"), 2);
    }
}
