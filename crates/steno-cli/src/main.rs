//! CLI `steno` : ask · doctor · init · export · mcp · skill, plus outils de développement
//! (replay, simulate, daemon) et le crochet `policy-hook` appelé par les agents.

mod init;
mod mcp;
mod replay;
mod serve;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use steno_core::core::CoreOptions;
use steno_core::events::CoreEvent;
use steno_core::live::SidecarSpec;
use steno_core::model::MeetingFilter;
use steno_core::{Core, Db};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "steno", version, about = "Sténo — assistant de réunion local", long_about = None)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Pose une question sur l'historique des réunions (ou une recherche croisée avec --croisee).
    Ask {
        question: String,
        /// Recherche croisée (code, Jira, Confluence, Datadog) au lieu de l'historique.
        #[arg(long)]
        croisee: bool,
        /// Réunion de rattachement (recherche croisée).
        #[arg(long)]
        meeting: Option<String>,
        /// N'affiche que les résultats plein texte, sans appel IA.
        #[arg(long)]
        no_ai: bool,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Vérifie l'installation : permissions, Teams, sidecar, agent, connecteurs, politique.
    Doctor {
        /// Interroge réellement l'agent pour tester la politique lecture seule (consomme un appel).
        #[arg(long)]
        live: bool,
    },
    /// Premier lancement en ligne de commande (équivalent de l'assistant de l'app).
    Init,
    /// Nouvelle version publiée ? `--install` : met à jour les sources et lance `make install`.
    Update {
        #[arg(long)]
        install: bool,
    },
    /// Exporte une réunion (ou toutes) : compte rendu complet en markdown, ou transcription
    /// seule (txt, sous-titres srt / vtt).
    Export {
        id: Option<String>,
        #[arg(long)]
        all: bool,
        /// md (or markdown), txt, srt, vtt.
        #[arg(long, default_value = "md")]
        format: String,
        /// Dossier de sortie (sinon : sortie standard pour une réunion).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Serveur MCP local en lecture seule (stdio).
    Mcp,
    /// Skill « Sténo » pour votre agent (Claude Code, Codex…) : réunions, comptes rendus,
    /// décisions et actions via `steno mcp`.
    Skill {
        #[command(subcommand)]
        action: SkillCmd,
    },
    /// Liste les réunions.
    Meetings {
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value_t = 30)]
        limit: i64,
    },
    /// Réglages : export / import TOML (config d'équipe).
    Settings {
        #[command(subcommand)]
        action: SettingsCmd,
    },
    /// Tâches en attente (et exécution si l'app ne tourne pas).
    Jobs {
        #[arg(long)]
        run: bool,
    },
    /// Simulateur du sidecar audio : rejoue un fichier d'exemple (protocole JSON lines).
    Replay {
        file: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        speed: f64,
        /// Quitte à la fin du fichier.
        #[arg(long)]
        exit: bool,
    },
    /// Rejoue un fichier d'exemple dans tout le pipeline (sans app) et attend le compte rendu.
    Simulate {
        file: PathBuf,
        #[arg(long, default_value_t = 0.0)]
        speed: f64,
        #[arg(long, default_value_t = 600)]
        timeout: u64,
    },
    /// Cœur sans interface : session en direct + file de tâches (journal sur la sortie standard).
    Daemon {
        /// Commande du sidecar (ex. : --sidecar steno --sidecar replay --sidecar fichier.jsonl).
        #[arg(long)]
        sidecar: Vec<String>,
    },
    /// Sert l'API de l'app en HTTP (+ l'interface web) : tests e2e et développement sans Tauri.
    Serve {
        #[arg(long, default_value_t = 4180)]
        port: u16,
        /// Dossier de l'interface compilée (app/ui/dist).
        #[arg(long)]
        ui: Option<PathBuf>,
    },
    /// Crochet « avant outil » des agents (lit l'appel sur stdin ; code 2 = refusé).
    #[command(hide = true)]
    PolicyHook { policy: PathBuf },
}

#[derive(Subcommand)]
enum SkillCmd {
    /// Copie le skill dans les dossiers de skills et déclare le serveur MCP `steno`.
    Install,
    /// Affiche où le skill et le serveur MCP sont installés.
    Status,
}

#[derive(Subcommand)]
enum SettingsCmd {
    Export { file: Option<PathBuf> },
    Import { file: PathBuf },
    Show,
}

fn open_core(sidecar: Option<SidecarSpec>) -> Result<Core> {
    steno_core::paths::ensure_dirs()?;
    let db = Db::open_default()?;
    Core::new(db, CoreOptions { hook_exe: std::env::current_exe().ok(), sidecar, agent_override: None })
}

fn print_request(r: &steno_core::model::Request) {
    match r.status.as_str() {
        "done" | "refused" => println!("{}", r.answer_md.clone().or(r.summary.clone()).unwrap_or_default()),
        "error" => eprintln!("❌ {}", r.error.clone().unwrap_or_default()),
        "cancelled" => eprintln!("Demande annulée."),
        s => eprintln!("Demande en cours ({s})…"),
    }
    if !r.tools_used.is_empty() {
        eprintln!("\nOutils utilisés : {}", r.tools_used.join(", "));
    }
}

async fn wait_request(core: &Core, id: &str, timeout: u64) -> Result<steno_core::model::Request> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
    loop {
        let r = core.db().get_request(id)?.context("demande introuvable")?;
        if !matches!(r.status.as_str(), "queued" | "running") {
            return Ok(r);
        }
        if tokio::time::Instant::now() > deadline {
            bail!("délai dépassé ; la demande continue dans l'app");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Policy hook: handled before any argument parsing, and always fail-closed (code 2 on
    // any error, never 1).
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if args.get(1).map(|a| a == "policy-hook").unwrap_or(false) {
        let policy = args.get(2).map(PathBuf::from).unwrap_or_default();
        std::process::exit(steno_core::policy::hook_entry(&policy));
    }
    let cli = Cli::parse();
    if let Cmd::Replay { file, speed, exit } = &cli.cmd {
        return replay::run(file, *speed, *exit);
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
        .with_writer(std::io::stderr)
        .init();

    match cli.cmd {
        Cmd::Ask { question, croisee, meeting, no_ai, timeout } => {
            let core = open_core(None)?;
            if !croisee {
                let hits = core.db().search_relevant(&question, 8)?;
                if hits.is_empty() {
                    eprintln!("Aucun extrait trouvé dans l'historique.");
                } else {
                    eprintln!("Extraits trouvés :");
                    for h in &hits {
                        eprintln!(
                            "  • {} ({}) : {}",
                            h.title.clone().unwrap_or_else(|| h.meeting_id.clone()),
                            h.started_at.get(..10).unwrap_or(""),
                            h.snippet.replace('\n', " ")
                        );
                    }
                    eprintln!();
                }
                if no_ai {
                    return Ok(());
                }
            }
            let r = if croisee {
                core.ask(&question, meeting.as_deref(), "cli")?
            } else {
                core.ask_history(&question, "cli")?
            };
            let r = if r.status == "queued" {
                if core.db().app_is_running()? {
                    eprintln!("L'app Sténo traite la demande…");
                } else {
                    let c = core.clone();
                    tokio::spawn(async move {
                        let _ = c.run_pending_jobs().await;
                    });
                }
                wait_request(&core, &r.id, timeout).await?
            } else {
                r
            };
            print_request(&r);
        }
        Cmd::Update { install } => {
            let core = open_core(None)?;
            let st = core.check_update().await?;
            println!("Version installée : {}", st.current);
            match (&st.latest, st.available) {
                (None, _) => println!("Aucune version publiée pour l'instant."),
                (Some(v), false) => println!("À jour (dernière version publiée : {v})."),
                (Some(v), true) => {
                    println!("Nouvelle version disponible : {v}{}", st.url.as_deref().map(|u| format!(" — {u}")).unwrap_or_default());
                    if install {
                        println!("Installation (quelques minutes, journal : {})…", steno_core::paths::logs_dir().join("update.log").display());
                        steno_core::updates::install(core.inner()).await?;
                        println!("Installé : relancez Sténo.");
                    } else if let Some(why) = &st.message {
                        println!("Installation automatique impossible : {why}");
                    } else {
                        println!("Pour l'installer : steno update --install");
                    }
                }
            }
        }
        Cmd::Doctor { live } => {
            let core = open_core(None)?;
            let items = steno_core::doctor::run(&core, live).await;
            let mut worst = 0;
            for i in &items {
                let icon = match i.status.as_str() {
                    "ok" => "✅",
                    "warn" => {
                        worst = worst.max(1);
                        "⚠️ "
                    }
                    _ => {
                        worst = 2;
                        "❌"
                    }
                };
                println!("{icon} {} : {}", i.name, i.message);
                if let Some(f) = &i.fix {
                    if i.status != "ok" {
                        println!("     → {f}");
                    }
                }
            }
            if worst == 2 {
                std::process::exit(1);
            }
        }
        Cmd::Init => {
            let core = open_core(None)?;
            init::run(&core).await?;
        }
        Cmd::Export { id, all, format, out } => {
            // `md` stays the CLI default; the core names it `markdown`.
            let format = if format == "md" { "markdown".to_string() } else { format };
            if !steno_core::export::FORMATS.contains(&format.as_str()) {
                bail!("format non pris en charge : {format} (md, txt, srt ou vtt)");
            }
            let core = open_core(None)?;
            let ids: Vec<String> = if all {
                core.db().list_meetings(&MeetingFilter { limit: Some(5000), ..Default::default() })?.into_iter().map(|m| m.meeting.id).collect()
            } else {
                vec![id.context("précisez un identifiant de réunion ou --all")?]
            };
            match out {
                Some(dir) => {
                    for id in &ids {
                        let p = steno_core::export::export_to_dir_as(core.db(), id, &dir, &format)?;
                        eprintln!("{}", p.display());
                    }
                }
                None if ids.len() == 1 => print!("{}", core.export_meeting_as(&ids[0], &format)?),
                None => bail!("--all demande --out <dossier>"),
            }
        }
        Cmd::Mcp => {
            // Read-only: no migration, no database creation, no write.
            let path = steno_core::paths::db_path();
            if !path.exists() {
                bail!("base Sténo introuvable ({}) : lancez Sténo une première fois", path.display());
            }
            mcp::serve(Db::open_read_only(&path)?)?;
        }
        Cmd::Skill { action } => {
            let st = match action {
                SkillCmd::Install => steno_core::agents::skill::install().await?,
                SkillCmd::Status => steno_core::agents::skill::status().await,
            };
            for t in &st.targets {
                let state = match t.state.as_str() {
                    "current" => "à jour",
                    "outdated" => "ancienne version",
                    _ => "absent",
                };
                println!("skill   {:<12} {} ({})", state, t.path, t.agents);
            }
            for m in &st.mcp {
                println!("mcp     {:<12} {}", if m.registered { "déclaré" } else { "absent" }, m.agent);
            }
            if st.targets.is_empty() && st.mcp.is_empty() {
                println!("aucun agent compatible trouvé (Claude Code, Codex…)");
            }
        }
        Cmd::Meetings { query, limit } => {
            let core = open_core(None)?;
            for m in core.db().list_meetings(&MeetingFilter { query, limit: Some(limit), ..Default::default() })? {
                println!(
                    "{}  {:<10} {}{}  {}",
                    m.meeting.started_at.get(..16).unwrap_or("").replace('T', " "),
                    m.meeting.status,
                    if m.meeting.confidential { "🔒 " } else { "" },
                    m.meeting.title.clone().unwrap_or_default(),
                    m.meeting.id
                );
            }
        }
        Cmd::Settings { action } => {
            let core = open_core(None)?;
            match action {
                SettingsCmd::Export { file } => {
                    let t = core.export_settings()?;
                    match file {
                        Some(f) => std::fs::write(f, t)?,
                        None => print!("{t}"),
                    }
                }
                SettingsCmd::Import { file } => {
                    core.import_settings(&std::fs::read_to_string(file)?)?;
                    eprintln!("Réglages importés.");
                }
                SettingsCmd::Show => println!("{}", serde_json::to_string_pretty(&core.settings())?),
            }
        }
        Cmd::Jobs { run } => {
            let core = open_core(None)?;
            for j in core.db().pending_jobs()? {
                println!("#{} {} {} ({}, essais {})", j.id, j.kind, j.meeting_id.unwrap_or_default(), j.status, j.attempts);
            }
            if run {
                if core.db().app_is_running()? {
                    bail!("l'app Sténo tourne : c'est elle qui traite la file");
                }
                let n = core.run_pending_jobs().await?;
                eprintln!("{n} tâche(s) exécutée(s).");
            }
        }
        Cmd::Simulate { file, speed, timeout } => simulate(file, speed, timeout).await?,
        Cmd::Serve { port, ui } => {
            // Sidecar : STENO_SIDECAR (ex. « steno replay fichier.jsonl --speed 20 »), sinon aucun.
            let spec = std::env::var("STENO_SIDECAR").ok().and_then(|c| {
                let parts: Vec<String> = c.split_whitespace().map(String::from).collect();
                parts.split_first().map(|(p, a)| SidecarSpec { program: p.clone(), args: a.to_vec() })
            });
            let core = open_core(spec)?;
            core.start_background();
            serve::run(core, port, ui).await?;
        }
        Cmd::Daemon { sidecar } => {
            let spec = (!sidecar.is_empty()).then(|| SidecarSpec { program: sidecar[0].clone(), args: sidecar[1..].to_vec() });
            let core = open_core(spec)?;
            core.start_background();
            let mut rx = core.subscribe();
            eprintln!("Sténo tourne sans interface (Ctrl+C pour arrêter).");
            loop {
                tokio::select! {
                    ev = rx.recv() => match ev {
                        Ok(ev) => log_event(&ev),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => break,
                    },
                    _ = tokio::signal::ctrl_c() => break,
                }
            }
            core.shutdown();
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Cmd::PolicyHook { .. } | Cmd::Replay { .. } => unreachable!(),
    }
    Ok(())
}

fn log_event(ev: &CoreEvent) {
    match ev {
        CoreEvent::Segment(s) => println!(
            "[{}] {}{}{} : {}",
            s.ts.get(11..19).unwrap_or(""),
            s.speaker,
            if s.private { " (aparté)" } else { "" },
            if s.is_echo { " (écho)" } else { "" },
            s.text
        ),
        CoreEvent::Request(r) => println!("» demande {} [{}] {} : {}", r.id.chars().take(8).collect::<String>(), r.intent, r.status, r.summary.clone().unwrap_or(r.query.clone())),
        CoreEvent::Report { meeting_id, version } => println!("» compte rendu v{version} : {meeting_id}"),
        CoreEvent::Notify { title, body, .. } => println!("🔔 {title} — {body}"),
        CoreEvent::CommandDetected { .. } => println!("🔔 commande vocale détectée"),
        CoreEvent::Update(u) => println!("⬆ mise à jour : {} {}", u.state, u.message.clone().unwrap_or_default()),
        CoreEvent::Live(_) | CoreEvent::MeetingsChanged | CoreEvent::SegmentDeleted { .. } | CoreEvent::SettingsChanged(_) => {}
    }
}

/// Pipeline complet sans app : replay → réunion → demandes → compte rendu.
async fn simulate(file: PathBuf, speed: f64, timeout: u64) -> Result<()> {
    let exe = std::env::current_exe()?;
    let spec = SidecarSpec {
        program: exe.display().to_string(),
        args: vec!["replay".into(), file.display().to_string(), "--speed".into(), speed.to_string()],
    };
    let core = open_core(Some(spec))?;
    let mut rx = core.subscribe();
    core.start_background();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
    // L'écoute ne démarre jamais seule : on la démarre comme le ferait l'utilisateur.
    while core.live_state().sidecar != "running" && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let m = core.start_meeting(None).await?;
    eprintln!("Écoute démarrée : {}", m.title.clone().unwrap_or_default());
    let mut meeting: Option<String> = None;
    let mut last_segment = tokio::time::Instant::now();
    let mut seen_segments = 0usize;
    let mut stopped = false;
    loop {
        if tokio::time::Instant::now() > deadline {
            bail!("délai dépassé");
        }
        if let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await {
            if let CoreEvent::Segment(s) = &ev {
                meeting = Some(s.meeting_id.clone());
                seen_segments += 1;
                last_segment = tokio::time::Instant::now();
            }
            if !matches!(ev, CoreEvent::Segment(_)) {
                log_event(&ev);
            }
        }
        // Fin du fichier : plus de segment depuis 3 s → on arrête la réunion.
        if !stopped && seen_segments > 0 && last_segment.elapsed() > Duration::from_secs(3) {
            stopped = true;
            eprintln!("{seen_segments} segments reçus ; fin de la réunion.");
            let _ = core.stop_meeting().await;
            // Simulation : pas de délai pour ajouter du contexte, le compte rendu part tout de suite.
            if let Some(id) = &meeting {
                let _ = core.report_now(id);
            }
        }
        if stopped {
            let Some(id) = &meeting else { break };
            let pending = core.db().pending_jobs()?;
            let m = core.db().get_meeting(id)?.context("réunion introuvable")?;
            let active = core.db().count_active_requests()?;
            if pending.is_empty() && active == 0 && matches!(m.status.as_str(), "done" | "error") {
                println!("\nRéunion : {} ({})", m.title.unwrap_or_default(), id);
                for r in core.db().requests(id)? {
                    println!("  • [{}] {} → {} : {}", r.intent, r.query, r.status, r.summary.unwrap_or_default());
                }
                if let Some(r) = core.db().get_report(id)? {
                    println!("\n{}", r.markdown);
                }
                break;
            }
        }
    }
    core.shutdown();
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(())
}
