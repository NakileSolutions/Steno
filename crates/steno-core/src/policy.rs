//! Politique lecture seule commune (PLAN §5.4) et crochet de contrôle des outils.
//!
//! La politique est décrite dans `agent/policy.toml` (intégré au binaire ; un fichier
//! `policy.toml` dans le dossier des données le remplace). Chaque adaptateur la traduit
//! dans le mécanisme de son CLI ; quand le CLI sait appeler un crochet avant chaque outil,
//! ce crochet est `<exécutable steno> policy-hook <fichier de politique>` (voir `hook_main`).

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

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
    /// Skills de l'utilisateur autorisés : outil `Skill` et lecture des fichiers d'un skill
    /// ou d'un plugin (consignes et références, jamais d'écriture).
    #[serde(default)]
    pub allow_skills: bool,
    /// Motifs exclus des dépôts (`Repo.exclude_globs`) : filtre sur les lectures directes de
    /// fichiers (chemin lu, fichier nommé dans une commande, motif de noms de fichiers).
    #[serde(default)]
    pub exclude_globs: Vec<String>,
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

/// Nom d'un serveur MCP tel qu'il apparaît dans les noms d'outils (`mcp__<nom>__outil`) :
/// « claude.ai Atlassian » → `claude_ai_Atlassian`.
pub fn mcp_server_id(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

/// Le fichier est-il dans le dossier d'un skill (un parent contient `SKILL.md`) ou d'un
/// plugin d'agent (un parent contient `.claude-plugin/plugin.json`) ? Le dossier personnel
/// et la racine ne comptent jamais comme dossier de skill.
pub fn in_skill_dir(path: &Path) -> bool {
    let home = dirs::home_dir().map(|h| h.canonicalize().unwrap_or(h));
    let mut dir = if path.is_dir() { Some(path) } else { path.parent() };
    let mut depth = 0;
    while let Some(d) = dir {
        if depth > 8 || d.parent().is_none() || Some(d) == home.as_deref() {
            return false;
        }
        if d.join("SKILL.md").is_file() || d.join(".claude-plugin").join("plugin.json").is_file() {
            return true;
        }
        dir = d.parent();
        depth += 1;
    }
    false
}

/// Motif d'exclusion d'un dépôt (insensible à la casse) appliqué à un chemin relatif au dépôt.
/// Sans `/`, le motif vise un nom à n'importe quel niveau (`.env*`, `*.pem`) ; avec `/`, un
/// chemin depuis la racine (`secrets/**`). Un dossier exclu exclut son contenu.
pub fn exclude_match(pattern: &str, rel: &str) -> bool {
    let p = pattern.trim().trim_end_matches('/');
    let rel = rel.trim_start_matches("./").trim_matches('/');
    if p.is_empty() || p.starts_with('#') || rel.is_empty() {
        return false;
    }
    if !p.contains('/') {
        return rel.split('/').any(|c| glob_match(p, c));
    }
    let p = p.trim_start_matches('/');
    let within = format!("{p}/*");
    let rooted = format!("/{rel}");
    [rel, rooted.as_str()].iter().any(|r| glob_match(p, r) || glob_match(&within, r))
}

/// Caractères refusés partout dans une commande, même entre guillemets.
const SHELL_META: &[&str] = &[";", "&", ">", "<", "`", "\n", "\r"];

/// Options qui lancent un programme ou écrivent un fichier, refusées pour toute commande même
/// si un `policy.toml` utilisateur plus ancien ne les liste pas. `grep` et `find` sont, sous
/// Claude Code, ugrep et bfs (`--filter`, `--pager`, `--view`, `--save-config`, `-rm`).
const MANDATORY_DENIED_ARGUMENTS: &[&str] = &[
    "-exec", "-execdir", "-ok", "-okdir", "-delete", "-rm", "-fls", "-fprint", "-fprint0", "-fprintf", "--output",
    "--pre", "--pre-glob", "--hostname-bin", "--compress-program", "--filter", "--pager", "--view", "--save-config",
    "--config", "--open-files-in-pager",
];

/// Règles propres à une commande : lettres d'options courtes cherchées dans tout le groupe, et
/// options refusées en plus de la liste commune.
fn command_rules(name: &str, sub: &str) -> (&'static str, &'static [&'static str]) {
    match (name, sub) {
        // `sort -o` écrit un fichier ; `git grep -O` ouvre les fichiers avec un programme (`-1Osh`).
        ("sort", _) => ("o", &[]),
        ("git", "grep") => ("O", &[]),
        // Options qui suivent les liens symboliques pendant un parcours (lecture hors des dépôts).
        ("rg", _) => ("L", &["--follow"]),
        ("grep", _) => ("RS", &["--dereference-recursive", "--dereference-files"]),
        ("ls", _) => ("L", &["--dereference"]),
        ("find", _) => ("", &["-L", "-follow"]),
        _ => ("", &[]),
    }
}

/// Options dont la valeur est un fichier lu (et parfois affiché, `git blame --ignore-revs-file`).
const FILE_OPTIONS: &[&str] = &[
    "--file", "--contents", "--ignore-file", "--ignore-revs-file", "--files-from", "--files0-from", "--exclude-from",
    "--include-from", "--pathspec-from-file",
];

/// Lettres d'options courtes suivies d'un fichier lu : `-f` (`rg`, `grep`, `git grep`) et, pour
/// `git blame`, `-S` (fichier de révisions, dont chaque ligne ressort dans les erreurs). `find`
/// n'a que des options d'un tiret en mots entiers.
fn file_letters(name: &str, sub: &str) -> &'static str {
    match (name, sub) {
        ("find", _) => "",
        ("git", "blame") => "fS",
        _ => "f",
    }
}

/// Valeur collée d'une option qui lit un fichier (`--contents=.env`, `-f.env`, `-nf/etc/x`,
/// `-S.env`), à contrôler comme un opérande.
fn file_option_value<'a>(t: &'a str, letters: &str) -> Option<&'a str> {
    if let Some(long) = t.strip_prefix("--") {
        let (name, value) = long.split_once('=')?;
        let file = !name.is_empty() && FILE_OPTIONS.iter().any(|o| o[2..].starts_with(name));
        return Some(value).filter(|v| file && !v.is_empty());
    }
    let i = t[1..].find(|c| letters.contains(c))? + 2;
    t.get(i..).filter(|v| !v.is_empty())
}

/// Découpe une commande en étapes de tube (`|` hors guillemets), puis en mots tels que le
/// programme les recevra (guillemets et barres obliques inverses retirés). Refuse ce que le
/// shell développerait ou exécuterait : `$` hors guillemets simples, `(` `)` hors guillemets,
/// `~utilisateur`, accolades, motifs pouvant désigner `..` (`.*`).
fn shell_words(cmd: &str) -> Result<Vec<Vec<String>>, String> {
    #[derive(PartialEq)]
    enum Q {
        No,
        Single,
        Double,
    }
    let mut parts: Vec<Vec<String>> = vec![vec![]];
    let mut w = String::new();
    let mut started = false;
    let mut q = Q::No;
    let mut chars = cmd.chars().peekable();
    let dollar = || Err("variable ou expansion du shell interdite (« $ »)".to_string());
    while let Some(c) = chars.next() {
        match q {
            Q::Single if c == '\'' => q = Q::No,
            Q::Single => w.push(c),
            Q::Double => match c {
                '"' => q = Q::No,
                '$' => return dollar(),
                '\\' => match chars.next() {
                    Some(n @ ('$' | '`' | '"' | '\\')) => w.push(n),
                    Some(n) => {
                        w.push('\\');
                        w.push(n);
                    }
                    None => return Err("guillemet non fermé".into()),
                },
                _ => w.push(c),
            },
            Q::No => match c {
                ' ' | '\t' | '|' => {
                    if started {
                        parts.last_mut().unwrap().push(std::mem::take(&mut w));
                        started = false;
                    }
                    if c == '|' {
                        parts.push(vec![]);
                    }
                }
                '\'' => {
                    q = Q::Single;
                    started = true;
                }
                '"' => {
                    q = Q::Double;
                    started = true;
                }
                '\\' => {
                    let Some(n) = chars.next() else { return Err("barre oblique inverse en fin de commande".into()) };
                    w.push(n);
                    started = true;
                }
                '$' => return dollar(),
                '(' | ')' => return Err(format!("caractère « {c} » interdit hors guillemets")),
                '{' if !w.ends_with('@') => return Err("accolades interdites hors guillemets".into()),
                '~' if !started && !matches!(chars.peek(), None | Some('/' | ' ' | '\t' | '|')) => {
                    return Err("« ~utilisateur » interdit".into());
                }
                '*' | '?' | '[' if w.rsplit('/').next().is_some_and(|comp| comp.starts_with('.')) => {
                    return Err(format!("motif pouvant désigner un dossier parent : {w}{c}…"));
                }
                _ => {
                    w.push(c);
                    started = true;
                }
            },
        }
    }
    if q != Q::No {
        return Err("guillemet non fermé".into());
    }
    if started {
        parts.last_mut().unwrap().push(w);
    }
    Ok(parts)
}

/// Le mot désigne-t-il un chemin à contrôler même s'il n'existe pas ?
fn looks_like_path(t: &str) -> bool {
    t.starts_with('/') || t.starts_with('~') || has_parent_component(t)
}

fn has_parent_component(t: &str) -> bool {
    t.split(['/', '\\']).any(|c| c == "..")
}

/// Partie fixe d'un motif de noms de fichiers (`src/**/*.rs` → `src`).
fn fixed_prefix(pattern: &str) -> String {
    let fixed: Vec<&str> = pattern.split('/').take_while(|c| !c.contains(['*', '?', '[', '{'])).collect();
    match fixed.join("/") {
        f if f.is_empty() && pattern.starts_with('/') => "/".into(),
        f if f.is_empty() => ".".into(),
        f => f,
    }
}

/// `git branch` : consultation seulement (ni création, ni suppression, ni renommage).
fn check_git_branch(args: &[&str]) -> Result<(), String> {
    const OPTIONS: &[&str] = &[
        "--list", "-l", "-a", "--all", "-r", "--remotes", "-v", "-vv", "--verbose", "--show-current", "--contains",
        "--no-contains", "--merged", "--no-merged", "--points-at", "--sort", "--format", "--color", "--no-color",
        "--column", "--no-column", "--ignore-case", "-i",
    ];
    const WITH_ARG: &[&str] = &["--contains", "--no-contains", "--merged", "--no-merged", "--points-at", "--sort", "--format"];
    let list = args.iter().any(|t| *t == "--list" || *t == "-l");
    let mut expect_arg = false;
    for t in args {
        if std::mem::take(&mut expect_arg) {
            continue;
        }
        if t.starts_with('-') {
            let name = t.split('=').next().unwrap_or(t);
            let short = t.strip_prefix('-').is_some_and(|g| !g.is_empty() && g.chars().all(|c| "alrvi".contains(c)));
            if !OPTIONS.contains(&name) && !short {
                return Err(format!("git branch : option non autorisée (consultation seulement) : {t}"));
            }
            expect_arg = WITH_ARG.contains(&name) && !t.contains('=');
        } else if !list {
            return Err(format!("git branch : création ou modification de branche interdite ({t})"));
        }
    }
    Ok(())
}

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
            allow_skills: false,
            exclude_globs: vec![],
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
        self.readable_dirs.iter().map(|d| d.canonicalize().unwrap_or_else(|_| d.clone())).collect()
    }

    /// Motif d'exclusion qui vise ce chemin relatif au dépôt, s'il y en a un.
    fn excluded_rel(&self, rel: &str) -> Option<&str> {
        self.exclude_globs.iter().map(String::as_str).find(|p| exclude_match(p, rel))
    }

    /// Motif d'exclusion visé par un chemin ou un motif de noms de fichiers (`**/.env*`,
    /// `*.pem`, `{.env,a}`) relatif à `base`. Aucun parcours du disque.
    fn excluded_pattern(&self, pattern: &str, base: &Path) -> Option<&str> {
        if self.exclude_globs.is_empty() {
            return None;
        }
        let sample = pattern.replace("**", "x").replace(['*', '?'], "x");
        let abs = resolve(&base.join(&sample));
        let rel = self.roots().iter().find_map(|r| abs.strip_prefix(r).ok().map(|p| p.to_string_lossy().into_owned()));
        self.excluded_rel(rel.as_deref().unwrap_or(&sample))
            .or_else(|| sample.split(['{', '}', ',']).find_map(|alt| self.excluded_rel(alt)))
    }

    /// Le chemin, une fois résolu (liens symboliques compris), est-il dans un dossier lisible et
    /// hors des fichiers exclus ?
    pub fn check_path(&self, path: &str, cwd: &Path) -> Result<(), String> {
        let expanded = if path == "~" {
            dirs::home_dir().unwrap_or_default()
        } else if let Some(rest) = path.strip_prefix("~/") {
            dirs::home_dir().unwrap_or_default().join(rest)
        } else if path.starts_with('~') {
            return Err(format!("chemin interdit : {path}"));
        } else {
            PathBuf::from(path)
        };
        let resolved = resolve(&cwd.join(expanded));
        let roots = self.roots();
        let Some(root) = roots.iter().find(|r| resolved.starts_with(r)) else {
            if self.allow_skills && in_skill_dir(&resolved) {
                return Ok(());
            }
            return Err(format!("lecture hors des dépôts autorisés : {}", resolved.display()));
        };
        let rel = resolved.strip_prefix(root).unwrap_or(&resolved).to_string_lossy();
        match self.excluded_rel(&rel) {
            Some(p) => Err(format!("fichier exclu par les réglages du dépôt (motif « {p} ») : {}", resolved.display())),
            None => Ok(()),
        }
    }

    /// Vérifie une commande shell : syntaxe, commande autorisée, options, puis chemins.
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
        self.check_path(".", cwd).map_err(|e| format!("dossier courant refusé ({e})"))?;
        for (i, words) in shell_words(cmd)?.iter().enumerate() {
            let toks: Vec<&str> = words.iter().map(String::as_str).collect();
            self.check_step(i, &toks, cwd)?;
        }
        Ok(())
    }

    fn check_step(&self, i: usize, toks: &[&str], cwd: &Path) -> Result<(), String> {
        let Some(&name) = toks.first() else { return Err("tube vide".into()) };
        let prefix_len = |a: &str| {
            let a: Vec<&str> = a.split_whitespace().collect();
            (toks.len() >= a.len() && toks[..a.len()] == a[..]).then_some(a.len())
        };
        let skip = if i == 0 {
            let n = self.base.allowed_commands.iter().filter_map(|a| prefix_len(a)).max();
            n.ok_or_else(|| format!("commande non autorisée : {}", toks.join(" ")))?
        } else if self.base.pipe_commands.iter().any(|p| p == name) {
            1
        } else {
            return Err(format!("commande non autorisée après un tube : {name}"));
        };
        let sub = if name == "git" { toks.get(1).copied().unwrap_or("") } else { "" };
        let (letters, extra) = command_rules(name, sub);
        let args = &toks[skip..];
        let mut operands = 0;
        let mut only_operands = false;
        for &t in args {
            if t == "--" && !only_operands {
                only_operands = true;
                continue;
            }
            let option = t.starts_with('-') && t != "-";
            // Refusée même après `--` : pour `find`, `--` ne termine que les options de tête et
            // l'expression qui suit s'exécute (`find -- . -delete`).
            if option && self.denied_option(t, letters, extra) {
                return Err(format!("option interdite : {t}"));
            }
            if option && !only_operands {
                if let Some(v) = file_option_value(t, file_letters(name, sub)) {
                    self.check_operand(v, false, cwd)?;
                }
            } else {
                operands += 1;
                self.check_operand(t, name == "git", cwd)?;
            }
        }
        match (name, sub) {
            ("git", "branch") => check_git_branch(args),
            ("uniq", _) if operands > 1 => Err("uniq : un seul fichier en argument (le second serait écrit)".into()),
            _ => Ok(()),
        }
    }

    /// Option refusée : forme exacte, `--opt=…`, abréviation (`--out`) ou lettre dans un groupe.
    fn denied_option(&self, t: &str, letters: &str, extra: &[&str]) -> bool {
        let mut denied = MANDATORY_DENIED_ARGUMENTS
            .iter()
            .chain(extra)
            .copied()
            .chain(self.base.denied_arguments.iter().map(String::as_str));
        if let Some(long) = t.strip_prefix("--") {
            let name = long.split('=').next().unwrap_or(long);
            name.starts_with('-') || (!name.is_empty() && denied.any(|d| d.strip_prefix("--").is_some_and(|d| d.starts_with(name))))
        } else {
            denied.any(|d| d == t) || t[1..].chars().any(|c| letters.contains(c))
        }
    }

    /// Opérande : chemin contrôlé s'il en a l'air ou s'il existe (liens symboliques résolus),
    /// et fichier ou motif nommé (`.env`, `*.pem`, `HEAD:.env` pour git) hors des exclusions.
    fn check_operand(&self, t: &str, git: bool, cwd: &Path) -> Result<(), String> {
        if looks_like_path(t) || cwd.join(t).symlink_metadata().is_ok() {
            self.check_path(t, cwd)?;
        }
        let rev_path = if git { t.split_once(':').map(|(_, p)| p) } else { None };
        let excluded = self.excluded_pattern(t, cwd).or_else(|| rev_path.and_then(|p| self.excluded_rel(p)));
        match excluded {
            Some(p) => Err(format!("fichier exclu par les réglages du dépôt (motif « {p} ») : {t}")),
            None => Ok(()),
        }
    }

    /// Décision pour un appel d'outil (`tool_name` au format de l'agent, `input` JSON).
    pub fn decide(&self, tool_name: &str, input: &serde_json::Value, cwd: &Path) -> Decision {
        if !self.tools_enabled {
            return Decision::Deny("aucun outil n'est autorisé pour cette tâche".into());
        }
        if let Some(rest) = tool_name.strip_prefix("mcp__") {
            let (server, tool) = rest.split_once("__").unwrap_or((rest, ""));
            if server.is_empty() || !self.allowed_mcp_servers.iter().any(|s| mcp_server_id(s) == server) {
                return Decision::Deny(format!("connecteur « {server} » non activé dans Sténo"));
            }
            if let Some(r) = self.mcp_tool_denied(tool) {
                return Decision::Deny(r);
            }
            return Decision::Allow;
        }
        let verdict = |r: Result<(), String>| match r {
            Ok(()) => Decision::Allow,
            Err(e) => Decision::Deny(e),
        };
        match tool_name {
            "Bash" => match input.get("command").and_then(|v| v.as_str()) {
                Some(c) => verdict(self.check_command(c, cwd)),
                None => Decision::Deny("commande absente".into()),
            },
            t if self.base.builtin_read_tools.iter().any(|b| b == t) => verdict(self.check_read_tool(t, input, cwd)),
            // Charger un skill ne fait que lire ses consignes ; ce qu'il fait ensuite passe
            // par les mêmes contrôles que le reste.
            "Skill" if self.allow_skills => Decision::Allow,
            other => Decision::Deny(format!("outil « {other} » non autorisé (lecture seule)")),
        }
    }

    /// Outils intégrés (`Read`, `Grep`, `Glob`, `LS`) : chemin explicite, sinon dossier courant,
    /// et motif de noms de fichiers (`pattern` de Glob, `glob` de Grep). `..` est refusé : Claude
    /// Code normalise ces chemins sans résoudre les liens symboliques.
    fn check_read_tool(&self, tool: &str, input: &serde_json::Value, cwd: &Path) -> Result<(), String> {
        let field = |k: &str| input.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty());
        let pattern = match tool {
            "Glob" => field("pattern"),
            "Grep" => field("glob"),
            _ => None,
        };
        for v in ["file_path", "path", "notebook_path"].into_iter().filter_map(field).chain(pattern) {
            if has_parent_component(v) {
                return Err(format!("« .. » interdit dans les chemins des outils intégrés : {v}"));
            }
        }
        let mut base = cwd.to_path_buf();
        let mut explicit = false;
        for k in ["file_path", "path", "notebook_path"] {
            if let Some(p) = field(k) {
                self.check_path(p, cwd)?;
                explicit = true;
                if k == "path" {
                    base = cwd.join(p);
                }
            }
        }
        if !explicit {
            self.check_path(".", cwd).map_err(|e| format!("dossier courant refusé ({e})"))?;
        }
        if let Some(p) = pattern {
            self.check_path(&fixed_prefix(p), &base)?;
            if let Some(e) = self.excluded_pattern(p, &base) {
                return Err(format!("motif visant des fichiers exclus (« {e} ») : {p}"));
            }
        }
        Ok(())
    }

    /// Incrémente le compteur sous verrou exclusif (appels d'outils parallèles) et refuse
    /// au-delà du budget.
    fn count_call(&self) -> Decision {
        if self.max_tool_calls == 0 {
            return Decision::Allow;
        }
        let Some(f) = &self.counter_file else { return Decision::Allow };
        match bump_counter(f) {
            Err(e) => Decision::Deny(format!("compteur d'appels d'outils illisible ({e})")),
            Ok(n) if n > self.max_tool_calls => Decision::Deny(format!(
                "budget de {} appels d'outils atteint : réponds avec ce que tu as et propose « approfondir après la réunion »",
                self.max_tool_calls
            )),
            Ok(_) => Decision::Allow,
        }
    }

    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        crate::paths::write_private(path, &serde_json::to_vec_pretty(self).unwrap_or_default())
    }

    pub fn read_from(path: &Path) -> anyhow::Result<ReadOnlyPolicy> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
}

/// Chemin absolu résolu comme le ferait le système : la partie existante la plus longue par
/// `canonicalize` (liens symboliques, `..`), le reste ajouté tel quel.
fn resolve(abs: &Path) -> PathBuf {
    let mut base = abs;
    let mut rest = PathBuf::new();
    loop {
        if let Ok(p) = base.canonicalize() {
            return lexical_normalize(&p.join(&rest));
        }
        match (base.parent(), base.file_name()) {
            (Some(parent), Some(name)) => {
                rest = Path::new(name).join(&rest);
                base = parent;
            }
            _ => return lexical_normalize(abs),
        }
    }
}

fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn bump_counter(path: &Path) -> std::io::Result<u32> {
    use std::io::{Read, Seek};
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    let n = s.trim().parse::<u32>().unwrap_or(0).saturating_add(1);
    f.seek(std::io::SeekFrom::Start(0))?;
    f.set_len(0)?;
    f.write_all(n.to_string().as_bytes())?;
    Ok(n)
}

/// Exécute le crochet en garantissant un refus (code 2) en cas d'erreur ou de panique : jamais
/// un autre code, que l'agent traiterait comme non bloquant.
fn fail_closed(f: impl FnOnce() -> i32) -> i32 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(0) => 0,
        Ok(_) => 2,
        Err(_) => {
            let _ = writeln!(std::io::stderr(), "Refusé par Sténo (lecture seule) : erreur interne du crochet");
            2
        }
    }
}

/// Point d'entrée de `<exécutable> policy-hook <fichier>` : lit l'appel sur l'entrée standard
/// (4 Mo au plus) puis décide (voir `hook_main`).
pub fn hook_entry(policy_path: &Path) -> i32 {
    fail_closed(|| {
        use std::io::Read;
        let mut buf = Vec::new();
        if std::io::stdin().take(4 * 1024 * 1024 + 1).read_to_end(&mut buf).is_err() || buf.len() > 4 * 1024 * 1024 {
            let _ = writeln!(std::io::stderr(), "Sténo : appel d'outil illisible, refusé");
            return 2;
        }
        hook_main(policy_path, &String::from_utf8_lossy(&buf))
    })
}

/// Crochet « avant outil » : lit l'appel en JSON (`tool_name`, `tool_input`, `cwd`), renvoie le
/// code de sortie (0 = autorisé, 2 = refusé, raison sur stderr). En cas de doute, on refuse.
pub fn hook_main(policy_path: &Path, stdin: &str) -> i32 {
    fail_closed(|| {
        let policy = match ReadOnlyPolicy::read_from(policy_path) {
            Ok(p) => p,
            Err(e) => {
                let _ = writeln!(std::io::stderr(), "Sténo : politique illisible ({e}), outil refusé");
                return 2;
            }
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(stdin) else {
            let _ = writeln!(std::io::stderr(), "Sténo : appel d'outil illisible, refusé");
            return 2;
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
            // Charger un skill ne compte pas dans le budget d'appels.
            Decision::Allow if tool == "Skill" => Decision::Allow,
            Decision::Allow => policy.count_call(),
            d => d,
        };
        match decision {
            Decision::Allow => 0,
            Decision::Deny(reason) => {
                let _ = writeln!(std::io::stderr(), "Refusé par Sténo (lecture seule) : {reason}");
                2
            }
        }
    })
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
            allow_skills: false,
            exclude_globs: vec![],
        }
    }

    fn accepted(p: &ReadOnlyPolicy, cwd: &Path, cmds: &[&str]) {
        for c in cmds {
            assert!(p.check_command(c, cwd).is_ok(), "{c} devrait passer : {:?}", p.check_command(c, cwd));
        }
    }

    fn refused(p: &ReadOnlyPolicy, cwd: &Path, cmds: &[&str]) {
        for c in cmds {
            assert!(p.check_command(c, cwd).is_err(), "{c} devrait être refusé");
        }
    }

    #[test]
    fn skills_utilisateur() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        let skill = d.path().join("skills").join("jira");
        let plugin = d.path().join("plugins").join("datadog");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(skill.join("references")).unwrap();
        std::fs::create_dir_all(plugin.join(".claude-plugin")).unwrap();
        std::fs::create_dir_all(plugin.join("skills").join("logs")).unwrap();
        std::fs::write(skill.join("SKILL.md"), "x").unwrap();
        std::fs::write(skill.join("references").join("jql.md"), "x").unwrap();
        std::fs::write(plugin.join(".claude-plugin").join("plugin.json"), "{}").unwrap();
        std::fs::write(plugin.join("skills").join("logs").join("notes.md"), "x").unwrap();
        let ailleurs = d.path().join("secret.txt");
        std::fs::write(&ailleurs, "x").unwrap();
        let read = |p: &ReadOnlyPolicy, f: &Path| p.decide("Read", &json!({"file_path": f}), &repo);

        let mut p = pol(&repo);
        assert!(matches!(p.decide("Skill", &json!({"skill": "jira"}), &repo), Decision::Deny(_)));
        assert!(matches!(read(&p, &skill.join("references").join("jql.md")), Decision::Deny(_)));

        p.allow_skills = true;
        assert_eq!(p.decide("Skill", &json!({"skill": "jira"}), &repo), Decision::Allow);
        assert_eq!(read(&p, &skill.join("references").join("jql.md")), Decision::Allow);
        assert_eq!(read(&p, &plugin.join("skills").join("logs").join("notes.md")), Decision::Allow);
        assert!(matches!(read(&p, &ailleurs), Decision::Deny(_)));
        // Les skills n'ouvrent ni l'écriture ni les commandes.
        assert!(matches!(p.decide("Write", &json!({"file_path": skill.join("x")}), &repo), Decision::Deny(_)));
        assert!(matches!(p.decide("Bash", &json!({"command": "python3 x.py"}), &repo), Decision::Deny(_)));
    }

    #[test]
    fn globs_et_mots() {
        assert!(glob_match("*send*", "slack_send_message"));
        assert!(!glob_match("*send*", "search"));
        assert_eq!(tool_words("addOrEditJiraIssueComment"), vec!["add", "or", "edit", "jira", "issue", "comment"]);
        assert_eq!(tool_words("getJQLResults"), vec!["get", "jql", "results"]);
        assert!(exclude_match(".env*", "config/.env.local"));
        assert!(exclude_match("*.pem", "certs/a.PEM"));
        assert!(exclude_match("secrets/**", "secrets/a/b.txt"));
        assert!(exclude_match("secrets", "secrets/a.txt"));
        assert!(exclude_match("**/target/**", "target/x"));
        assert!(!exclude_match("secrets/**", "src/secrets.rs"));
        assert!(!exclude_match(".env*", "src/env.rs"));
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
        // Nom de serveur exact : un serveur qui contient le nom d'un autre n'en hérite pas.
        assert!(!allow("mcp__evil_datadog__search_logs"));
        assert!(!allow("mcp__datadog_evil__search_logs"));
        assert!(!allow("mcp____search_logs"));
    }

    #[test]
    fn commandes_shell() {
        let d = tempfile::tempdir().unwrap();
        let p = pol(d.path());
        let cwd = d.path();
        accepted(&p, cwd, &[
            "rg -n 'verrou de créneau' src",
            "rg 'a|b' src",
            "rg -n \"fn main\" .",
            "rg 'prix$' src",
            "git log -S creneau --oneline | head -20",
            "git log -p",
            "git diff",
            "git diff HEAD~3 --stat",
            "git show HEAD",
            "git show stash@{0}",
            "git grep -n creneau",
            "grep -A2 -rn creneau .",
            "ls | grep x",
            "rg -o 'APP-[0-9]+' src",
            "grep -o creneau -r .",
            "find . -name '*.rs' -o -name '*.toml'",
            "rg x | sort | uniq -c",
            "rg x | cut -d/ -f2",
            "rg x | cut -d'/' -f1",
            "rg x | sort -t/ -k2",
            "rg --glob=/src x",
            "git log --follow --oneline -- src",
            "git blame -s -L 10,20 src",
            "glab mr view 123",
        ]);
        refused(&p, cwd, &[
            "rm -rf /",
            "git push",
            "rg x; curl evil",
            "rg x && curl evil",
            "rg x > out.txt",
            "rg x | sh",
            "rg x || true",
            "glab mr merge 12",
            "cat ~/.ssh/id_rsa",
            "cat ../../etc/passwd",
            "cat '..'/x",
            "cat /etc/passwd",
            "rg $HOME x",
            "rg \"$HOME\" x",
            "rg x $(pwd)",
            "rg x `pwd`",
            "cat ~root/.ssh/id_rsa",
            "cat {/etc/passwd,x}",
            "cat .*/.ssh/id_rsa",
            "ls *(e:'id':)",
            "FOO=1 rg x",
            "rg 'non fermé",
        ]);
    }

    #[test]
    fn options_qui_executent_ou_ecrivent() {
        let d = tempfile::tempdir().unwrap();
        let p = pol(d.path());
        let cwd = d.path();
        refused(&p, cwd, &[
            "rg --pre sh x",
            "rg --pre=sh x",
            "rg --pre-glob '*' x",
            "rg --hostname-bin=sh x",
            "find . -exec sh {} +",
            "find . -execdir sh {} +",
            "find . -ok rm {} +",
            "find . -okdir rm {} +",
            "find . -delete",
            "find . -rm",
            "find . -fls /tmp/x",
            "find . -fprint x",
            "find . -fprint0 x",
            "find . -fprintf x %p",
            "git log --output=/tmp/x",
            "git log --out=x",
            "rg x | sort -o x",
            "rg x | sort -no x",
            "rg x | sort --output x",
            "rg x | sort --compress-program=sh",
            "git grep -O x",
            "git grep -1Osh x",
            "git grep --open-files-in-pager=sh x",
            "grep --filter='*:sh' x .",
            "grep --pager=sh x .",
            "grep ---evil x .",
            "rg x | uniq - out.txt",
            "rg -f/etc/passwd x",
            "rg -nf/etc/passwd x .",
            "rg --ignore-file=/etc/passwd x",
            "git blame --ignore-revs-file=/etc/passwd x",
            "git blame -S/etc/passwd x",
            "find -- . -delete",
            "find -- . -fprint out",
            "rg -L x",
            "rg --follow x",
            "grep -R x .",
            "grep -rS x .",
            "grep --dereference-recursive x .",
            "find -L .",
            "find . -follow",
            "ls -LR",
        ]);
        let mut old = p.clone();
        old.base.denied_arguments = vec!["-exec".into()];
        refused(&old, cwd, &["rg --pre=sh x", "find . -delete"]);
    }

    #[test]
    fn git_branch_en_consultation() {
        let d = tempfile::tempdir().unwrap();
        let p = pol(d.path());
        let cwd = d.path();
        accepted(&p, cwd, &[
            "git branch",
            "git branch --list 'feat/*'",
            "git branch -a",
            "git branch -r",
            "git branch -vv",
            "git branch --show-current",
            "git branch --contains abc123",
            "git branch --merged main",
            "git branch --sort=-committerdate",
        ]);
        refused(&p, cwd, &[
            "git branch nouvelle",
            "git branch -d feat",
            "git branch -D feat",
            "git branch -m a b",
            "git branch -f main HEAD~1",
            "git branch --set-upstream-to=origin/x",
            "git branch --contains abc nouvelle",
        ]);
    }

    #[test]
    fn exclusions_des_depots() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(repo.join("secrets")).unwrap();
        std::fs::write(repo.join(".env"), "CLE=1").unwrap();
        std::fs::write(repo.join("src").join("main.rs"), "x").unwrap();
        std::fs::write(repo.join("secrets").join("a.txt"), "x").unwrap();
        let mut p = pol(repo);
        p.exclude_globs = vec![".env*".into(), "*.pem".into(), "secrets/**".into()];
        let tool = |t: &str, input: serde_json::Value| p.decide(t, &input, repo);
        assert!(matches!(tool("Read", json!({"file_path": repo.join(".env")})), Decision::Deny(_)));
        assert!(matches!(tool("Read", json!({"file_path": repo.join("secrets").join("a.txt")})), Decision::Deny(_)));
        assert!(matches!(tool("Glob", json!({"pattern": "**/.env*"})), Decision::Deny(_)));
        assert!(matches!(tool("Glob", json!({"pattern": "**/{.env,x}"})), Decision::Deny(_)));
        assert!(matches!(tool("Glob", json!({"pattern": "secrets/*"})), Decision::Deny(_)));
        assert!(matches!(tool("Grep", json!({"pattern": "CLE", "glob": ".env*"})), Decision::Deny(_)));
        assert_eq!(tool("Read", json!({"file_path": repo.join("src").join("main.rs")})), Decision::Allow);
        assert_eq!(tool("Glob", json!({"pattern": "**/*.rs"})), Decision::Allow);
        assert_eq!(tool("Grep", json!({"pattern": "CLE"})), Decision::Allow);
        refused(&p, repo, &[
            "cat .env",
            "rg CLE .env",
            "git show HEAD:.env",
            "cat *.pem",
            "cat secrets/a.txt",
            "git log -p -- .env",
            "git blame --contents=.env -- src/main.rs",
            "git blame --ignore-revs-file=.env src/main.rs",
            "git blame -S.env src/main.rs",
            "rg -f.env x",
        ]);
        accepted(&p, repo, &["rg CLE", "git log -p", "git diff", "git show HEAD", "grep -A2 -rn CLE src", "ls | grep env", "cat src/main.rs"]);
    }

    #[test]
    fn liens_symboliques_et_dossier_courant() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        let dehors = d.path().join("dehors");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&dehors).unwrap();
        std::fs::write(dehors.join("secret.txt"), "x").unwrap();
        std::os::unix::fs::symlink(&dehors, repo.join("lien")).unwrap();
        let p = pol(&repo);
        refused(&p, &repo, &[
            "cat lien/secret.txt",
            "rg x lien",
            "git blame --contents=lien/secret.txt -- src",
            "git blame --ignore-revs-file=lien/secret.txt -- src",
            "git blame -Slien/secret.txt -- src",
        ]);
        assert!(matches!(p.decide("Read", &json!({"file_path": repo.join("lien").join("secret.txt")}), &repo), Decision::Deny(_)));
        assert!(matches!(p.decide("Read", &json!({"file_path": repo.join("lien").join("..").join("x")}), &repo), Decision::Deny(_)));
        assert!(matches!(p.decide("Glob", &json!({"pattern": "/etc/*"}), &repo), Decision::Deny(_)));
        assert!(matches!(p.decide("Glob", &json!({"pattern": "../*"}), &repo), Decision::Deny(_)));
        // Dossier courant hors des dépôts : ni commande ni recherche sans chemin.
        refused(&p, &dehors, &["rg x"]);
        assert!(matches!(p.decide("Grep", &json!({"pattern": "x"}), &dehors), Decision::Deny(_)));
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
        assert_eq!(p.decide("Grep", &json!({"pattern": "a..b"}), d.path()), Decision::Allow, "regex, pas un chemin");
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
        assert_eq!(hook_main(&d.path().join("absente.json"), &call), 2);
        assert_eq!(fail_closed(|| panic!("boum")), 2, "panique : refus");
        assert_eq!(fail_closed(|| 1), 2, "jamais un code non bloquant");
    }

    #[test]
    fn budget_tenu_sous_appels_paralleles() {
        let d = tempfile::tempdir().unwrap();
        let mut p = pol(d.path());
        p.max_tool_calls = 10;
        p.counter_file = Some(d.path().join("count"));
        let allowed: usize = std::thread::scope(|s| {
            let hs: Vec<_> = (0..40).map(|_| s.spawn(|| p.count_call() == Decision::Allow)).collect();
            hs.into_iter().map(|h| usize::from(h.join().unwrap())).sum()
        });
        assert_eq!(allowed, 10);
    }
}
