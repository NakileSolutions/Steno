//! Façade du cœur : état partagé (`Inner`) et API utilisée par l'app et la CLI.

use crate::agents::{self, AgentAdapter, AgentEvent, RunRequest, RunResult, SessionMode, ToolAccess};
use crate::db::Db;
use crate::events::{CoreEvent, LiveState};
use crate::live::{LiveCmd, LiveHandle, SidecarSpec};
use crate::model::*;
use crate::prompts::Sources;
use crate::settings::Settings;
use crate::{now_iso, policy::ReadOnlyPolicy};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::{broadcast, mpsc, Notify};
use tokio_util::sync::CancellationToken;

/// Options de construction du cœur.
#[derive(Clone, Default)]
pub struct CoreOptions {
    /// Exécutable qui répond à `policy-hook` (l'app ou la CLI elle-même).
    pub hook_exe: Option<PathBuf>,
    /// Sidecar audio à lancer (absent : pas de capture, demandes texte uniquement).
    pub sidecar: Option<SidecarSpec>,
    /// Adaptateur imposé (tests).
    pub agent_override: Option<Arc<dyn AgentAdapter>>,
}

/// État partagé entre l'app, la file de tâches et la session en direct.
pub struct Inner {
    pub db: Db,
    events: broadcast::Sender<CoreEvent>,
    settings: RwLock<Settings>,
    pub hook_exe: Option<PathBuf>,
    pub jobs_notify: Notify,
    pub cancels: Mutex<HashMap<String, CancellationToken>>,
    meeting_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    agent_override: Option<Arc<dyn AgentAdapter>>,
    pub live_state: Mutex<LiveState>,
    pub live: Mutex<Option<LiveHandle>>,
    pub sidecar_spec: Option<SidecarSpec>,
    pub shutdown: CancellationToken,
}

impl Sources {
    pub fn repo_paths(&self) -> Vec<PathBuf> {
        self.repos.iter().map(|(_, p)| PathBuf::from(p)).collect()
    }
}

impl Inner {
    pub fn emit(&self, e: CoreEvent) {
        let _ = self.events.send(e);
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn adapter(&self) -> Result<Arc<dyn AgentAdapter>> {
        if let Some(a) = &self.agent_override {
            return Ok(a.clone());
        }
        let s = self.settings();
        agents::adapter_for(&s.agent.provider, &s.agent.custom_command)
            .with_context(|| format!("agent inconnu : {}", s.agent.provider))
    }

    /// Sources cochées pour le direct (`live = true`) ou la corrélation.
    pub fn sources(&self, live: bool) -> Result<Sources> {
        let s = self.settings();
        let provider = self.adapter()?.id().to_string();
        let repos = self
            .db
            .list_repos()?
            .into_iter()
            .filter(|r| if live { r.use_live } else { r.use_correlation })
            .filter(|r| std::path::Path::new(&r.path).is_dir())
            .map(|r| (if r.label.is_empty() { r.path.clone() } else { r.label.clone() }, r.path))
            .collect();
        let connectors = self
            .db
            .list_connectors(Some(&provider))?
            .into_iter()
            .filter(|c| c.write_blockable && c.status != "absent")
            .filter(|c| if live { c.use_live } else { c.use_correlation })
            .map(|c| c.name)
            .collect();
        Ok(Sources {
            repos,
            connectors,
            budget: if live { s.agent.live_tool_budget } else { s.correlation.budget },
        })
    }

    fn meeting_lock(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.meeting_locks
            .lock()
            .unwrap()
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Lance l'agent choisi. `SessionMode::New("")` = « session de la réunion » (reprise si
    /// l'agent sait le faire).
    #[allow(clippy::too_many_arguments)]
    pub async fn run_agent(
        &self,
        meeting: Option<&Meeting>,
        tier: Tier,
        tools: ToolAccess,
        session: SessionMode,
        prompt: String,
        budget: u32,
        repos: Option<Vec<PathBuf>>,
    ) -> Result<RunResult> {
        self.run_agent_with(meeting, tier, tools, session, prompt, budget, repos, CancellationToken::new())
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_agent_with(
        &self,
        meeting: Option<&Meeting>,
        tier: Tier,
        mut tools: ToolAccess,
        session: SessionMode,
        prompt: String,
        budget: u32,
        repos: Option<Vec<PathBuf>>,
        cancel: CancellationToken,
    ) -> Result<RunResult> {
        if meeting.map(|m| m.confidential).unwrap_or(false) {
            bail!("réunion confidentielle : aucun appel IA");
        }
        let s = self.settings();
        let adapter = self.adapter()?;
        let caps = adapter.capabilities();
        if let ToolAccess::ReadOnly { connectors } = &mut tools {
            if !caps.tool_deny_list && !connectors.is_empty() {
                tracing::warn!("{} ne sait pas bloquer les outils MCP en écriture : connecteurs retirés", adapter.id());
                connectors.clear();
            }
        }
        let key = meeting.map(|m| m.id.clone()).unwrap_or_else(|| "_global".into());
        let run_dir = crate::paths::runs_dir().join(&key);
        let instructions = crate::instructions::build(&s);
        let mut base_policy = ReadOnlyPolicy::no_tools();
        base_policy.readable_dirs = vec![run_dir.clone()];
        agents::prepare_run_dir(adapter.as_ref(), &run_dir, &instructions, &base_policy).await?;

        let uses_meeting_session = matches!(&session, SessionMode::New(id) if id.is_empty());
        let _guard;
        let session = if uses_meeting_session {
            match meeting {
                Some(m) if caps.resume_session => {
                    let lock = self.meeting_lock(&m.id);
                    _guard = lock.lock_owned().await;
                    // Relire : une exécution précédente a pu enregistrer la session.
                    let fresh = self.db.get_meeting(&m.id)?.unwrap_or_else(|| m.clone());
                    match (&fresh.agent_session_id, &fresh.agent_provider) {
                        (Some(id), Some(p)) if p == adapter.id() => SessionMode::Resume(id.clone()),
                        _ => SessionMode::New(uuid::Uuid::new_v4().to_string()),
                    }
                }
                _ => SessionMode::None,
            }
        } else {
            session
        };
        let repos = repos.unwrap_or_default();
        let timeout = s.agent.timeout_for(tier);
        let req = RunRequest {
            run_dir,
            prompt,
            tier,
            model: s.agent.model_for(tier),
            session: session.clone(),
            repos,
            tools,
            max_tool_calls: budget,
            timeout,
            allow_api_key: s.agent.allow_api_key,
            run_id: uuid::Uuid::new_v4().simple().to_string()[..12].to_string(),
            hook_exe: self.hook_exe.clone(),
        };
        let (tx, mut rx) = mpsc::channel::<AgentEvent>(256);
        tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                if let AgentEvent::ToolCall { name } = ev {
                    tracing::info!("outil appelé : {name}");
                }
            }
        });
        let mut res = adapter.run(req, tx, cancel).await;
        // Session perdue (purgée par le CLI) : on recommence une fois avec une nouvelle session.
        if let (Err(e), SessionMode::Resume(_), Some(m)) = (&res, &session, meeting) {
            let msg = e.to_string().to_lowercase();
            if msg.contains("session") && (msg.contains("not found") || msg.contains("introuvable") || msg.contains("no conversation")) {
                self.db.set_agent_session(&m.id, adapter.id(), None)?;
                bail!("la session de l'agent a expiré ; relancez la demande");
            }
        }
        if let (Ok(r), Some(m)) = (&mut res, meeting) {
            if uses_meeting_session {
                if let Some(sid) = r.session_id.clone().or(match &session {
                    SessionMode::New(id) | SessionMode::Resume(id) => Some(id.clone()),
                    SessionMode::None => None,
                }) {
                    self.db.set_agent_session(&m.id, adapter.id(), Some(&sid))?;
                }
            }
        }
        res
    }

    pub fn set_live_state(&self, f: impl FnOnce(&mut LiveState)) {
        let st = {
            let mut s = self.live_state.lock().unwrap();
            f(&mut s);
            s.clone()
        };
        self.emit(CoreEvent::Live(st));
    }

    pub fn send_live(&self, cmd: LiveCmd) -> bool {
        if let Some(h) = self.live.lock().unwrap().as_ref() {
            return h.send(cmd);
        }
        false
    }
}

/// Mots qui, en tête de demande, signifient « publier à l'extérieur ».
fn is_publish_request(query: &str) -> bool {
    let n = crate::text::normalize(query);
    if n.contains("brouillon") {
        return false;
    }
    let first_words: Vec<&str> = n.split(' ').take(2).collect();
    const VERBS: &[&str] = &[
        "poste", "postes", "publie", "envoie", "envoies", "partage", "cree", "crees", "ouvre", "commente", "ferme",
        "transitionne", "merge", "fusionne", "deploie", "assigne", "supprime", "reponds", "transfere", "notifie",
    ];
    first_words.iter().any(|w| VERBS.contains(w))
        || ["sur slack", "dans slack", "sur teams", "par mail", "par e-mail", "par email"].iter().any(|p| n.contains(p))
}

/// Cœur de Scribe (clonable).
#[derive(Clone)]
pub struct Core {
    pub inner: Arc<Inner>,
}

impl Core {
    pub fn new(db: Db, opts: CoreOptions) -> Result<Core> {
        let settings = Settings::load(&db)?;
        let (tx, _) = broadcast::channel(1024);
        Ok(Core {
            inner: Arc::new(Inner {
                db,
                events: tx,
                settings: RwLock::new(settings),
                hook_exe: opts.hook_exe,
                jobs_notify: Notify::new(),
                cancels: Mutex::new(HashMap::new()),
                meeting_locks: Mutex::new(HashMap::new()),
                agent_override: opts.agent_override,
                live_state: Mutex::new(LiveState::default()),
                live: Mutex::new(None),
                sidecar_spec: opts.sidecar,
                shutdown: CancellationToken::new(),
            }),
        })
    }

    pub fn db(&self) -> &Db {
        &self.inner.db
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.inner.events.subscribe()
    }

    /// Démarre les tâches de fond : file de tâches, battement de cœur, maintenance,
    /// session en direct (sidecar).
    pub fn start_background(&self) {
        let _ = self.inner.db.recover_jobs();
        crate::jobs::spawn_runner(self.inner.clone());
        crate::maintenance::spawn(self.inner.clone());
        let handle = crate::live::spawn(self.inner.clone());
        *self.inner.live.lock().unwrap() = Some(handle);
    }

    pub fn shutdown(&self) {
        self.inner.send_live(LiveCmd::Shutdown);
        self.inner.shutdown.cancel();
    }

    // ------------------------------------------------------------------ réglages

    pub fn settings(&self) -> Settings {
        self.inner.settings()
    }

    pub fn save_settings(&self, s: Settings) -> Result<Settings> {
        let s = s.validated()?;
        s.save(&self.inner.db)?;
        *self.inner.settings.write().unwrap() = s.clone();
        self.inner.send_live(LiveCmd::SettingsChanged);
        Ok(s)
    }

    pub fn export_settings(&self) -> Result<String> {
        self.settings().to_team_toml()
    }

    pub fn import_settings(&self, toml: &str) -> Result<Settings> {
        let s = self.settings().merge_team_toml(toml)?;
        self.save_settings(s)
    }

    // ------------------------------------------------------------------ direct

    pub fn live_state(&self) -> LiveState {
        let mut s = self.inner.live_state.lock().unwrap().clone();
        s.pending_requests = self.inner.db.count_active_requests().unwrap_or(0);
        if let Some(m) = &s.meeting {
            if let Ok(Some(fresh)) = self.inner.db.get_meeting(&m.id) {
                s.meeting = Some(fresh);
            }
        }
        s
    }

    /// Démarre une réunion manuellement.
    pub async fn start_meeting(&self, title: Option<String>) -> Result<Meeting> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if !self.inner.send_live(LiveCmd::Start { title, reply: tx }) {
            bail!("la session en direct n'est pas démarrée");
        }
        rx.await.context("session en direct arrêtée")?
    }

    pub async fn stop_meeting(&self) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if !self.inner.send_live(LiveCmd::Stop { reply: tx }) {
            bail!("la session en direct n'est pas démarrée");
        }
        rx.await.context("session en direct arrêtée")?
    }

    pub fn set_aside(&self, on: bool) {
        self.inner.send_live(LiveCmd::Aside(on));
    }

    pub fn set_live_confidential(&self, on: bool) -> Result<()> {
        if let Some(m) = self.inner.db.live_meeting()? {
            self.set_confidential(&m.id, on)?;
        } else {
            self.inner.send_live(LiveCmd::NextConfidential(on));
        }
        Ok(())
    }

    /// Enregistre un énoncé de test du mot d'appel (micro, N secondes).
    pub async fn wake_test_record(&self, seconds: u32) -> Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if !self.inner.send_live(LiveCmd::WakeTest { seconds, reply: tx }) {
            bail!("la capture audio n'est pas disponible");
        }
        rx.await.context("session en direct arrêtée")?
    }

    // ------------------------------------------------------------------ demandes

    /// Demande tapée (ou CLI). `meeting_id` absent : réunion en cours s'il y en a une.
    pub fn ask(&self, query: &str, meeting_id: Option<&str>, origin: &str) -> Result<Request> {
        let q = query.trim();
        if q.is_empty() {
            bail!("la demande est vide");
        }
        let meeting_id = match meeting_id {
            Some(id) => Some(id.to_string()),
            None => self.inner.db.live_meeting()?.map(|m| m.id),
        };
        // Un verbe en tête choisit l'intention (« note … », « vérifie … »), sinon recherche.
        let det = crate::wake::Detector::new(&self.settings().voice);
        let (intent, query, verb) = match det.detect_aside(q) {
            Some(d) if d.verb.is_some() => (d.intent, if d.query.is_empty() { q.to_string() } else { d.query }, d.verb),
            _ => (Intent::Search, q.to_string(), None),
        };
        create_request(&self.inner, meeting_id.as_deref(), origin, intent, &query, verb.as_deref(), &[])
    }

    /// Question sur l'historique des réunions.
    pub fn ask_history(&self, query: &str, origin: &str) -> Result<Request> {
        create_request(&self.inner, None, origin, Intent::History, query.trim(), None, &[])
    }

    pub fn cancel_request(&self, id: &str) -> Result<()> {
        cancel_request(&self.inner, id)
    }

    pub fn recent_requests(&self, limit: i64) -> Result<Vec<Request>> {
        self.inner.db.recent_requests(limit)
    }

    // ------------------------------------------------------------------ réunions

    pub fn set_confidential(&self, id: &str, on: bool) -> Result<()> {
        self.inner.db.set_confidential(id, on)?;
        if on {
            // Annule les demandes en attente de cette réunion.
            for r in self.inner.db.requests(id)? {
                if r.status == "queued" || r.status == "running" {
                    let _ = cancel_request(&self.inner, &r.id);
                }
            }
        }
        self.inner.emit(CoreEvent::MeetingsChanged);
        let mut st = self.inner.live_state.lock().unwrap().clone();
        if let Some(m) = &mut st.meeting {
            if m.id == id {
                m.confidential = on;
                drop(st);
                self.inner.set_live_state(|s| {
                    if let Some(m) = &mut s.meeting {
                        m.confidential = on;
                    }
                });
            }
        }
        Ok(())
    }

    pub fn rename_meeting(&self, id: &str, title: &str) -> Result<()> {
        self.inner.db.rename_meeting(id, title.trim())?;
        self.inner.emit(CoreEvent::MeetingsChanged);
        Ok(())
    }

    pub fn delete_meeting(&self, id: &str) -> Result<()> {
        self.inner.db.delete_meeting(id)?;
        let _ = std::fs::remove_dir_all(crate::paths::runs_dir().join(id));
        let _ = std::fs::remove_dir_all(crate::paths::cache_dir().join(id));
        self.inner.emit(CoreEvent::MeetingsChanged);
        Ok(())
    }

    pub fn regenerate_report(&self, id: &str) -> Result<()> {
        let m = self.inner.db.get_meeting(id)?.context("réunion introuvable")?;
        if m.confidential {
            bail!("réunion confidentielle : aucun appel IA");
        }
        if m.status == "live" {
            bail!("la réunion est encore en cours");
        }
        self.inner.db.enqueue_job("report", Some(id), &serde_json::json!({}))?;
        self.inner.jobs_notify.notify_one();
        Ok(())
    }

    pub fn correlate(&self, id: &str) -> Result<()> {
        let m = self.inner.db.get_meeting(id)?.context("réunion introuvable")?;
        if let Some(r) = crate::correlate::blocked_reason(&self.inner, &m) {
            bail!("Corrélation impossible : {r}");
        }
        if self.inner.db.get_report(id)?.is_none() {
            bail!("aucun compte rendu à corréler");
        }
        if self.inner.db.has_job("correlate", id, &["pending", "running"])? {
            bail!("une corrélation est déjà en cours");
        }
        self.inner.db.enqueue_job("correlate", Some(id), &serde_json::json!({}))?;
        self.inner.jobs_notify.notify_one();
        Ok(())
    }

    pub fn export_meeting(&self, id: &str) -> Result<String> {
        crate::export::meeting_markdown(&self.inner.db, id)
    }

    pub fn add_note(&self, meeting_id: &str, text: &str) -> Result<Note> {
        self.inner.db.add_note(meeting_id, text.trim(), "user")
    }

    // ------------------------------------------------------------------ agents

    pub async fn detect_agents(&self) -> Vec<agents::AgentInfo> {
        agents::detect_all(&self.settings().agent.custom_command).await
    }

    /// Envoie une invite de test à un agent.
    pub async fn test_agent(&self, provider: &str) -> Result<(bool, String, u128)> {
        let s = self.settings();
        let adapter = agents::adapter_for(provider, &s.agent.custom_command).context("agent inconnu")?;
        let run_dir = crate::paths::runs_dir().join("_test");
        let instructions = crate::instructions::build(&s);
        agents::prepare_run_dir(adapter.as_ref(), &run_dir, &instructions, &ReadOnlyPolicy::no_tools()).await?;
        let start = std::time::Instant::now();
        let (tx, _rx) = mpsc::channel(64);
        let r = adapter
            .run(
                RunRequest {
                    run_dir,
                    prompt: crate::prompts::agent_test(),
                    tier: Tier::Quick,
                    model: s.agent.model_for(Tier::Quick),
                    session: SessionMode::None,
                    repos: vec![],
                    tools: ToolAccess::None,
                    max_tool_calls: 0,
                    timeout: std::time::Duration::from_secs(120),
                    allow_api_key: s.agent.allow_api_key,
                    run_id: "test".into(),
                    hook_exe: self.inner.hook_exe.clone(),
                },
                tx,
                CancellationToken::new(),
            )
            .await;
        let ms = start.elapsed().as_millis();
        Ok(match r {
            Ok(r) => {
                let ok = !r.answer.trim().is_empty();
                (ok, crate::report::extract_summary(&r.answer), ms)
            }
            Err(e) => (false, e.to_string(), ms),
        })
    }

    /// Relit les connecteurs MCP de l'agent choisi.
    pub async fn refresh_connectors(&self) -> Result<Vec<Connector>> {
        let adapter = self.inner.adapter()?;
        let servers = adapter.list_mcp().await?;
        let list: Vec<(String, String)> = servers.into_iter().map(|s| (s.name, s.status)).collect();
        self.inner
            .db
            .sync_connectors(adapter.id(), &list, adapter.capabilities().tool_deny_list)?;
        self.inner.db.list_connectors(Some(adapter.id()))
    }

    pub fn list_connectors(&self) -> Result<Vec<Connector>> {
        let provider = self.inner.adapter()?.id().to_string();
        self.inner.db.list_connectors(Some(&provider))
    }

    // ------------------------------------------------------------------ mot d'appel

    pub fn check_wake_word(&self, word: &str, strict: bool) -> Result<crate::wake::check::WakeCheck> {
        crate::wake::check::check_wake_word(word, strict, Some(&self.inner.db))
    }

    pub fn generate_variants(&self, word: &str) -> crate::wake::variants::VariantCheck {
        let vocab = self.settings().audio.vocabulary;
        crate::wake::variants::check(word, &crate::wake::variants::generate(word), &vocab)
    }

    pub fn check_variants(&self, word: &str, variants: &[String]) -> crate::wake::variants::VariantCheck {
        crate::wake::variants::check(word, variants, &self.settings().audio.vocabulary)
    }

    pub fn wake_detect(&self, text: &str) -> Option<crate::wake::Detection> {
        crate::wake::Detector::new(&self.settings().voice).detect(text)
    }

    // ------------------------------------------------------------------ divers

    pub fn delete_all_data(&self) -> Result<()> {
        self.inner.db.delete_all()?;
        let _ = std::fs::remove_dir_all(crate::paths::runs_dir());
        let _ = std::fs::remove_dir_all(crate::paths::cache_dir());
        let _ = crate::paths::ensure_dirs();
        self.inner.emit(CoreEvent::MeetingsChanged);
        Ok(())
    }

    /// Exécute toutes les tâches en attente (CLI quand l'app ne tourne pas).
    pub async fn run_pending_jobs(&self) -> Result<usize> {
        crate::jobs::drain(self.inner.clone()).await
    }

    /// Ajoute une réunion terminée à partir d'un fichier d'exemple (démo, tests).
    pub fn now() -> String {
        now_iso()
    }
}

/// Crée une demande et, si besoin, la tâche correspondante.
pub(crate) fn create_request(
    inner: &Inner,
    meeting_id: Option<&str>,
    origin: &str,
    intent: Intent,
    query: &str,
    verb: Option<&str>,
    segment_ids: &[i64],
) -> Result<Request> {
    let meeting = match meeting_id {
        Some(id) => inner.db.get_meeting(id)?,
        None => None,
    };
    let s = inner.settings();
    let mut r = Request {
        id: uuid::Uuid::new_v4().to_string(),
        meeting_id: meeting.as_ref().map(|m| m.id.clone()),
        created_at: now_iso(),
        origin: origin.into(),
        intent: intent.as_str().into(),
        query: query.into(),
        context_seconds: matches!(intent, Intent::Search | Intent::Check).then_some(s.agent.live_context_seconds as i64),
        tier: intent.tier().as_str().into(),
        status: "queued".into(),
        started_at: None,
        finished_at: None,
        error: None,
        summary: None,
        answer_md: None,
        tools_used: vec![],
    };
    let finish = |r: &mut Request, status: &str, summary: String, answer: String| {
        r.status = status.into();
        r.finished_at = Some(now_iso());
        r.summary = Some(summary);
        r.answer_md = Some(answer);
    };
    match intent {
        Intent::Cancel => {
            match inner.db.last_active_request(meeting_id)? {
                Some(last) => {
                    cancel_request(inner, &last.id)?;
                    finish(&mut r, "done", format!("Demande annulée : {}", crate::text::truncate(&last.query, 80)), "Demande annulée.".into());
                }
                None => finish(&mut r, "done", "Aucune demande en cours.".into(), "Aucune demande en cours.".into()),
            }
        }
        Intent::Stop => {
            let live = inner.db.live_meeting()?;
            match live {
                Some(_) => {
                    inner.send_live(LiveCmd::StopRequested);
                    finish(&mut r, "done", "⏹ Écoute arrêtée.".into(), "Écoute arrêtée. Le compte rendu est en préparation.".into());
                }
                None => finish(&mut r, "done", "Aucune écoute en cours.".into(), "Aucune écoute en cours.".into()),
            }
        }
        Intent::Note => {
            let Some(m) = &meeting else { bail!("aucune réunion en cours pour cette note") };
            let is_action = verb.map(|v| crate::text::normalize(v).contains("action")).unwrap_or(false)
                || crate::text::normalize(query).starts_with("action");
            let mut text = query.trim_start_matches([':', ' ']).to_string();
            for p in ["que ", "qu'", "qu’"] {
                if text.to_lowercase().starts_with(p) {
                    text = text[p.len()..].trim_start().to_string();
                    break;
                }
            }
            if let Some(first) = text.chars().next() {
                text = first.to_uppercase().collect::<String>() + &text[first.len_utf8()..];
            }
            inner.db.add_note(&m.id, &text, if origin == "voice" || origin == "hotkey" { "voice" } else { "user" })?;
            if is_action {
                inner.db.add_action(&m.id, None, &text, None)?;
            }
            let label = if is_action { "✅ Action notée" } else { "📝 Noté" };
            finish(&mut r, "done", format!("{label} : {}", crate::text::truncate(&text, 120)), text.clone());
        }
        _ => {
            if meeting.as_ref().map(|m| m.confidential).unwrap_or(false) {
                r.status = "refused".into();
                r.error = Some("réunion confidentielle : aucun appel IA".into());
                r.summary = Some("Réunion confidentielle : aucune recherche.".into());
            } else if is_publish_request(query) {
                finish(
                    &mut r,
                    "refused",
                    "Je ne publie rien à l'extérieur.".into(),
                    "Je ne publie rien à l'extérieur (Slack, Jira, Confluence, GitLab, Datadog…). \
                     Demandez plutôt « rédige un brouillon de … » : vous le publierez vous-même."
                        .into(),
                );
                r.status = "refused".into();
            }
        }
    }
    inner.db.insert_request(&r)?;
    if !segment_ids.is_empty() {
        inner.db.link_segments_to_request(segment_ids, &r.id)?;
    }
    inner.emit(CoreEvent::Request(r.clone()));
    if r.status == "queued" {
        inner.db.enqueue_job("request", r.meeting_id.as_deref(), &serde_json::json!({ "request_id": r.id }))?;
        inner.jobs_notify.notify_one();
        inner.emit(CoreEvent::Notify {
            title: "🔎 Je cherche…".into(),
            body: crate::text::truncate(query, 120),
            route: None,
        });
    } else if let Some(sum) = &r.summary {
        inner.emit(CoreEvent::Notify { title: "Scribe".into(), body: sum.clone(), route: None });
    }
    Ok(r)
}

pub(crate) fn cancel_request(inner: &Inner, id: &str) -> Result<()> {
    if let Some(tok) = inner.cancels.lock().unwrap().remove(id) {
        tok.cancel();
    }
    if let Some(r) = inner.db.get_request(id)? {
        if r.status == "queued" || r.status == "running" {
            inner.db.finish_request(id, "cancelled", Some("Demande annulée."), None, &[], None)?;
            if let Some(r) = inner.db.get_request(id)? {
                inner.emit(CoreEvent::Request(r));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refus_de_publication() {
        assert!(is_publish_request("Poste ça sur Slack"));
        assert!(is_publish_request("crée le ticket pour ce bug"));
        assert!(is_publish_request("envoie le récap par mail à Paul"));
        assert!(!is_publish_request("rédige un brouillon de message Slack"));
        assert!(!is_publish_request("qui a créé le ticket APP-1234 ?"));
        assert!(!is_publish_request("les erreurs de ms-booking depuis hier"));
    }
}
