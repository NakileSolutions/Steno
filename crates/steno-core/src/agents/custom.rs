//! Adaptateur générique : modèle de commande saisi dans les réglages.
//!
//! Variables : `{prompt_file}` (fichier contenant l'invite complète), `{run_dir}`, `{model}`.
//! Sans `{prompt_file}`, l'invite est envoyée sur l'entrée standard. La sortie standard est
//! la réponse. Aucun contrôle des outils : ne pas y brancher de connecteurs.

use super::*;
use crate::agents::process::run_streaming;

pub struct Custom {
    template: String,
}

impl Custom {
    pub fn new(template: &str) -> Custom {
        Custom { template: template.trim().to_string() }
    }
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
pub(crate) fn render(template: &str, prompt_file: &Path, run_dir: &Path, model: Option<&str>) -> String {
    render_full(template, prompt_file, run_dir, model, None)
}

pub(crate) fn render_full(template: &str, prompt_file: &Path, run_dir: &Path, model: Option<&str>, effort: Option<&str>) -> String {
    template
        .replace("{effort}", &sh_quote(effort.unwrap_or("")))
        .replace("{prompt_file}", &sh_quote(&prompt_file.display().to_string()))
        .replace("{run_dir}", &sh_quote(&run_dir.display().to_string()))
        .replace("{model}", &sh_quote(model.unwrap_or("")))
}

/// Commande `sh -c <ligne>` lancée dans le dossier d'exécution, avec tout l'environnement :
/// l'utilisateur a écrit la commande, qui peut appeler un outil lisant sa propre clé d'API.
fn command(line: &str, req: &RunRequest) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(line).current_dir(&req.run_dir);
    cmd
}

#[async_trait]
impl AgentAdapter for Custom {
    fn id(&self) -> &'static str {
        "custom"
    }
    fn display_name(&self) -> &'static str {
        "Commande personnalisée"
    }
    fn support(&self) -> Support {
        Support::Generic
    }
    async fn detect(&self) -> Detection {
        if self.template.is_empty() {
            return Detection { installed: false, error: Some("aucune commande saisie".into()), ..Default::default() };
        }
        let first = self.template.split_whitespace().next().unwrap_or("");
        match which::which(first) {
            Ok(p) => Detection { installed: true, version: None, path: Some(p.display().to_string()), error: None },
            Err(_) => Detection { installed: false, error: Some(format!("« {first} » introuvable")), ..Default::default() },
        }
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { resume_session: false, stream_json: false, read_only_files: false, tool_deny_list: false, extra_dirs: false }
    }
    async fn list_mcp(&self) -> Result<Vec<McpServer>> {
        Ok(vec![])
    }
    async fn sync_instructions(&self, run_dir: &Path, instructions: &Instructions) -> Result<()> {
        std::fs::create_dir_all(run_dir)?;
        std::fs::write(run_dir.join("INSTRUCTIONS.md"), &instructions.markdown)?;
        Ok(())
    }
    async fn apply_policy(&self, _policy: &ReadOnlyPolicy, _run_dir: &Path) -> Result<()> {
        Ok(())
    }
    async fn run(&self, req: RunRequest, _events: mpsc::Sender<AgentEvent>, cancel: CancellationToken) -> Result<RunResult> {
        if self.template.is_empty() {
            anyhow::bail!("aucune commande personnalisée n'est configurée");
        }
        if matches!(&req.tools, ToolAccess::ReadOnly { connectors } if !connectors.is_empty()) {
            anyhow::bail!("la commande personnalisée ne peut pas utiliser de connecteurs");
        }
        std::fs::create_dir_all(req.run_dir.join(".steno"))?;
        let instructions = std::fs::read_to_string(req.run_dir.join("INSTRUCTIONS.md")).unwrap_or_default();
        let full = format!("{instructions}\n\n---\n\n{}", req.prompt);
        let pf = req.run_dir.join(".steno").join(format!("prompt-{}.md", req.run_id));
        crate::paths::write_private(&pf, full.as_bytes())?;
        let line = render_full(&self.template, &pf, &req.run_dir, req.model.as_deref(), req.effort.as_deref());
        let stdin = (!self.template.contains("{prompt_file}")).then(|| full.clone());
        let cmd = command(&line, &req);
        let out = run_streaming(cmd, stdin, req.timeout, cancel, |_| {}).await;
        let _ = std::fs::remove_file(&pf);
        let out = out?;
        if out.cancelled {
            anyhow::bail!("annulée");
        }
        if !out.success() {
            anyhow::bail!("{}", out.error_message("La commande personnalisée"));
        }
        Ok(RunResult { answer: out.stdout.trim().to_string(), model: req.model.clone(), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modele_de_commande() {
        let s = render("mon-ia --model {model} --input {prompt_file}", Path::new("/tmp/p.md"), Path::new("/r"), Some("x"));
        assert_eq!(s, "mon-ia --model 'x' --input '/tmp/p.md'");
    }

    #[tokio::test]
    async fn execution() {
        let d = tempfile::tempdir().unwrap();
        let c = Custom::new("sed -n 's/^DEMANDE: //p'");
        c.sync_instructions(d.path(), &Instructions { markdown: "# Consignes".into() }).await.unwrap();
        let (tx, _rx) = mpsc::channel(8);
        let r = c
            .run(
                RunRequest {
                    run_dir: d.path().to_path_buf(),
                    prompt: "DEMANDE: bonjour".into(),
                    tier: Tier::Quick,
                    model: None,
                    effort: None,
                    session: SessionMode::None,
                    repos: vec![],
                    exclude_globs: vec![],
                    tools: ToolAccess::None,
                    max_tool_calls: 0,
                    timeout: Duration::from_secs(5),
                    allow_api_key: false,
                    run_id: "t".into(),
                    hook_exe: None,
                    skills: false,
                },
                tx,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(r.answer, "bonjour");
    }

    #[test]
    fn environnement_complet() {
        let req = RunRequest {
            run_dir: PathBuf::from("/tmp"),
            prompt: String::new(),
            tier: Tier::Quick,
            model: None,
            effort: None,
            session: SessionMode::None,
            repos: vec![],
            exclude_globs: vec![],
            tools: ToolAccess::None,
            max_tool_calls: 0,
            timeout: Duration::from_secs(5),
            allow_api_key: false,
            run_id: "t".into(),
            hook_exe: None,
            skills: false,
        };
        let c = command("mon-ia", &req);
        assert_eq!(c.as_std().get_envs().filter(|(_, v)| v.is_none()).count(), 0, "aucune variable retirée");
    }
}
