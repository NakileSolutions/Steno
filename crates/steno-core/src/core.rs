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

/// `runtime` key changed on every settings save (see `Inner::reload_settings_if_changed`).
const SETTINGS_REV: &str = "settings_rev";

/// État partagé entre l'app, la file de tâches et la session en direct.
pub struct Inner {
    pub db: Db,
    events: broadcast::Sender<CoreEvent>,
    settings: RwLock<Settings>,
    /// Revision of the saved settings this process last loaded or wrote (`settings_rev` in
    /// `runtime`): another process (the CLI) changing it means the settings must be reloaded.
    settings_rev: Mutex<Option<String>>,
    pub hook_exe: Option<PathBuf>,
    pub jobs_notify: Notify,
    pub cancels: Mutex<HashMap<String, CancellationToken>>,
    /// Réunions dont le compte rendu doit partir sans attendre (« Rédiger maintenant » cliqué
    /// avant que la tâche de rédaction n'existe).
    pub report_now: Mutex<std::collections::HashSet<String>>,
    /// État des mises à jour (voir `updates`).
    pub update: Mutex<crate::updates::UpdateStatus>,
    /// Derniers événements du calendrier reçus du sidecar.
    pub calendar_cache: Mutex<Vec<crate::planning::CalendarEvent>>,
    meeting_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    agent_override: Option<Arc<dyn AgentAdapter>>,
    pub live_state: Mutex<LiveState>,
    pub live: Mutex<Option<LiveHandle>>,
    /// Dernière liste de périphériques reçue du sidecar.
    pub audio_devices: Mutex<crate::sidecar::AudioDevices>,
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

    fn apply_settings(&self, s: Settings) {
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = s.clone();
        self.send_live(LiveCmd::SettingsChanged);
        self.emit(CoreEvent::SettingsChanged(Box::new(s)));
    }

    /// Reloads the settings when another process (`steno settings import`) saved them, so
    /// the next save from the UI does not overwrite them. True if they were reloaded.
    pub fn reload_settings_if_changed(&self) -> Result<bool> {
        let rev = self.db.get_runtime(SETTINGS_REV)?.map(|(v, _)| v);
        {
            let mut seen = self.settings_rev.lock().unwrap_or_else(|e| e.into_inner());
            if *seen == rev {
                return Ok(false);
            }
            *seen = rev;
        }
        self.apply_settings(Settings::load(&self.db)?);
        Ok(true)
    }

    pub fn adapter(&self) -> Result<Arc<dyn AgentAdapter>> {
        if let Some(a) = &self.agent_override {
            return Ok(a.clone());
        }
        let s = self.settings();
        agents::adapter_for(&s.agent.provider, &s.agent.custom_command)
            .with_context(|| format!("agent inconnu : {}", s.agent.provider))
    }

    /// Agent choisi pour un usage (Réglages → Agent IA → Modèles et réflexion).
    pub fn adapter_for_tier(&self, tier: Tier) -> Result<Arc<dyn AgentAdapter>> {
        if let Some(a) = &self.agent_override {
            return Ok(a.clone());
        }
        let s = self.settings();
        let id = s.agent.agent_for(tier);
        agents::adapter_for(&id, &s.agent.custom_command).with_context(|| format!("agent inconnu : {id}"))
    }

    /// Sources cochées pour le direct (`live = true`) ou la corrélation (connecteurs de l'agent
    /// des recherches).
    pub fn sources(&self, live: bool) -> Result<Sources> {
        let s = self.settings();
        let provider = self.adapter_for_tier(Tier::Search)?.id().to_string();
        let repos = self
            .db
            .list_repos()?
            .into_iter()
            .filter(|r| if live { r.use_live } else { r.use_correlation })
            .filter(|r| std::path::Path::new(&r.path).is_dir())
            .map(|r| (if r.label.is_empty() { r.path.clone() } else { r.label.clone() }, r.path))
            .collect::<Vec<(String, String)>>();
        let projects = repos.iter().flat_map(|(_, p)| crate::prompts::find_projects(std::path::Path::new(p))).collect();
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
            projects,
            connectors,
            budget: if live { s.agent.live_tool_budget } else { s.correlation.budget },
        })
    }

    /// Motifs exclus (`Repo.exclude_globs`, un par ligne ou séparés par des virgules) des
    /// dépôts lisibles d'une exécution.
    fn exclude_globs_for(&self, repos: &[PathBuf]) -> Vec<String> {
        let Ok(all) = self.db.list_repos() else { return vec![] };
        let mut out: Vec<String> = vec![];
        for r in all.iter().filter(|r| repos.iter().any(|p| p == std::path::Path::new(&r.path))) {
            for g in r.exclude_globs.split(['\n', ',']).map(str::trim).filter(|g| !g.is_empty()) {
                if !out.iter().any(|x| x == g) {
                    out.push(g.to_string());
                }
            }
        }
        out
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
        let adapter = self.adapter_for_tier(tier)?;
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
        let prompt = crate::instructions::expand(&prompt, &s);
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
        let exclude_globs = self.exclude_globs_for(&repos);
        let timeout = s.agent.timeout_for(tier);
        let req = RunRequest {
            run_dir,
            prompt,
            tier,
            model: s.agent.model_for(tier),
            effort: s.agent.effort_for(tier),
            session: session.clone(),
            repos,
            exclude_globs,
            tools,
            max_tool_calls: budget,
            timeout,
            allow_api_key: s.agent.allow_api_key,
            run_id: uuid::Uuid::new_v4().simple().to_string()[..12].to_string(),
            hook_exe: self.hook_exe.clone(),
            skills: s.agent.use_skills,
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
        self.emit(CoreEvent::Live(self.localized_live(st)));
    }

    /// État du direct publié : dernière erreur dans la langue des réglages.
    pub(crate) fn localized_live(&self, mut st: LiveState) -> LiveState {
        if let Some(e) = &st.last_error {
            st.last_error = Some(crate::i18n_messages::localize(self.lang(), e));
        }
        st
    }

    pub fn send_live(&self, cmd: LiveCmd) -> bool {
        if let Some(h) = self.live.lock().unwrap().as_ref() {
            return h.send(cmd);
        }
        false
    }
}

/// Mots qui, en tête de demande, signifient « publier à l'extérieur ».
///
/// French and English requests are both checked, whatever the interface language: people
/// often speak the other language to the wake word.
fn is_publish_request(query: &str) -> bool {
    let n = crate::text::normalize(query);
    let words: Vec<&str> = n.split(' ').collect();
    if n.contains("brouillon") || words.iter().any(|w| *w == "draft" || *w == "drafts") {
        return false;
    }
    const VERBS: &[&str] = &[
        "poste", "postes", "publie", "envoie", "envoies", "partage", "cree", "crees", "ouvre", "commente", "ferme",
        "transitionne", "merge", "fusionne", "deploie", "assigne", "supprime", "reponds", "transfere", "notifie",
    ];
    // English imperatives. Words that are also common French words ("comment", "transition",
    // "message") or common read queries ("open issues") are left out on purpose.
    const VERBS_EN: &[&str] = &[
        "post", "publish", "send", "share", "create", "close", "deploy", "assign", "delete", "reply", "respond",
        "forward", "notify",
    ];
    // Polite English openings before the verb: "can you please post this on Slack".
    const LEAD_EN: &[&[&str]] =
        &[&["please"], &["pls"], &["kindly"], &["can", "you"], &["could", "you"], &["would", "you"], &["will", "you"]];
    let mut start = 0;
    while let Some(lead) = LEAD_EN.iter().find(|l| words[start..].starts_with(l)) {
        start += lead.len();
    }
    let is_verb = |i: usize| {
        let (w, next) = (words.get(i).copied().unwrap_or(""), words.get(i + 1).copied().unwrap_or(""));
        // Nouns, not verbs: "merge requests from yesterday", "post mortem of the incident".
        if (w == "merge" && next.starts_with("request")) || (w == "post" && next == "mortem") {
            return false;
        }
        VERBS.contains(&w) || VERBS_EN.contains(&w)
    };
    is_verb(0)
        || is_verb(1)
        || (start > 0 && is_verb(start))
        || ["sur slack", "dans slack", "sur teams", "par mail", "par e-mail", "par email"].iter().any(|p| n.contains(p))
        || ["by email", "by e-mail", "by mail", "via email", "via e-mail", "via slack", "via teams"]
            .iter()
            .any(|p| n.contains(p))
}

/// Cœur de Sténo (clonable).
#[derive(Clone)]
pub struct Core {
    pub inner: Arc<Inner>,
}

impl Core {
    pub fn new(db: Db, opts: CoreOptions) -> Result<Core> {
        let settings = Settings::load(&db)?;
        let settings_rev = db.get_runtime(SETTINGS_REV)?.map(|(v, _)| v);
        let (tx, _) = broadcast::channel(1024);
        Ok(Core {
            inner: Arc::new(Inner {
                db,
                events: tx,
                settings: RwLock::new(settings),
                settings_rev: Mutex::new(settings_rev),
                hook_exe: opts.hook_exe,
                jobs_notify: Notify::new(),
                cancels: Mutex::new(HashMap::new()),
                report_now: Mutex::new(Default::default()),
                update: Mutex::new(crate::updates::initial_status()),
                calendar_cache: Mutex::new(vec![]),
                meeting_locks: Mutex::new(HashMap::new()),
                agent_override: opts.agent_override,
                live_state: Mutex::new(LiveState::default()),
                live: Mutex::new(None),
                audio_devices: Mutex::new(Default::default()),
                sidecar_spec: opts.sidecar,
                shutdown: CancellationToken::new(),
            }),
        })
    }

    pub fn db(&self) -> &Db {
        &self.inner.db
    }

    /// État partagé du cœur (tâches longues lancées hors de l'app, ex. `steno update`).
    pub fn inner(&self) -> Arc<Inner> {
        self.inner.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.inner.events.subscribe()
    }

    /// Démarre les tâches de fond : file de tâches, battement de cœur, maintenance,
    /// session en direct (sidecar).
    pub fn start_background(&self) {
        let _ = self.inner.db.recover_jobs();
        // Read before our own heartbeat overwrites it: another running process (the app while
        // `steno serve` starts) owns the meeting still marked live.
        let other_alive = self.inner.db.other_process_running().unwrap_or(false);
        crate::jobs::spawn_runner(self.inner.clone());
        crate::maintenance::spawn(self.inner.clone());
        crate::updates::spawn(self.inner.clone());
        let handle = crate::live::spawn(self.inner.clone(), !other_alive);
        *self.inner.live.lock().unwrap() = Some(handle);
    }

    pub fn shutdown(&self) {
        self.inner.send_live(LiveCmd::Shutdown);
        self.inner.shutdown.cancel();
    }

    // ------------------------------------------------------------------ mises à jour

    pub fn update_status(&self) -> crate::updates::UpdateStatus {
        self.inner.update_status()
    }

    pub async fn check_update(&self) -> Result<crate::updates::UpdateStatus> {
        crate::updates::check(&self.inner).await
    }

    /// Lance l'installation en arrière-plan (quelques minutes) ; l'avancement arrive par
    /// `steno://update`.
    pub fn install_update(&self) -> Result<()> {
        let st = self.inner.update_status();
        if !st.available {
            bail!("aucune nouvelle version à installer");
        }
        if self.inner.db.live_meeting()?.is_some() {
            bail!("une réunion est en cours : installez la mise à jour après la réunion");
        }
        // Appelée hors du moteur asynchrone (commande synchrone), `tokio::spawn` paniquerait et
        // arrêterait l'app : on refuse proprement.
        let rt = tokio::runtime::Handle::try_current()
            .map_err(|_| anyhow::anyhow!("installation impossible hors du moteur asynchrone de l'app"))?;
        let inner = self.inner.clone();
        rt.spawn(async move {
            if let Err(e) = crate::updates::install(inner).await {
                tracing::warn!("mise à jour : {e:#}");
            }
        });
        Ok(())
    }

    // ------------------------------------------------------------------ réglages

    pub fn settings(&self) -> Settings {
        self.inner.settings()
    }

    pub fn save_settings(&self, s: Settings) -> Result<Settings> {
        let s = s.validated()?;
        s.save(&self.inner.db)?;
        let rev = uuid::Uuid::new_v4().to_string();
        self.inner.db.set_runtime(SETTINGS_REV, &rev)?;
        *self.inner.settings_rev.lock().unwrap() = Some(rev);
        self.inner.apply_settings(s.clone());
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

    // ------------------------------------------------------------------ permissions

    /// Dernier état connu des permissions, complété des informations système (serveur son,
    /// session graphique, raccourcis globaux). Redemande l'état au sidecar : la lecture suivante
    /// reflète un changement fait entre-temps dans les réglages du système.
    pub fn permissions(&self) -> crate::permissions::Permissions {
        self.inner.send_live(LiveCmd::RefreshPermissions);
        let mut p = self
            .inner
            .db
            .get_runtime("permissions")
            .ok()
            .flatten()
            .and_then(|(s, _)| serde_json::from_str::<crate::permissions::Permissions>(&s).ok())
            .unwrap_or_default();
        p.available = self.inner.live_state.lock().unwrap().sidecar == "running";
        crate::permissions::with_system_info(p, &self.inner.db)
    }

    /// Affiche l'invite macOS du droit (sans effet s'il a déjà été accordé ou refusé). Sous
    /// Linux, le sidecar revérifie seulement l'accès (il n'y a pas d'invite).
    pub fn request_permission(&self, kind: &str) -> Result<()> {
        crate::permissions::check_kind(kind)?;
        if !self.inner.send_live(LiveCmd::RequestPermission(kind.to_string())) {
            if cfg!(target_os = "macos") {
                anyhow::bail!("la capture audio n'est pas démarrée : ouvrez les Réglages Système");
            }
            anyhow::bail!("la capture audio n'est pas démarrée");
        }
        Ok(())
    }

    pub fn open_permission_settings(&self, kind: &str) -> Result<()> {
        crate::permissions::open_settings(kind)
    }

    /// L'assistant de premier lancement est terminé (ou passé) : il ne s'ouvrira plus.
    pub fn complete_onboarding(&self) -> Result<()> {
        self.inner.db.set_runtime("onboarded", "1")
    }

    pub fn onboarded(&self) -> Result<bool> {
        Ok(self.inner.db.get_runtime("onboarded")?.is_some())
    }

    pub fn live_state(&self) -> LiveState {
        let mut s = self.inner.live_state.lock().unwrap().clone();
        s.pending_requests = self.inner.db.count_active_requests().unwrap_or(0);
        if let Some(m) = &s.meeting {
            if let Ok(Some(fresh)) = self.inner.db.get_meeting(&m.id) {
                s.meeting = Some(fresh);
            }
        }
        self.inner.localized_live(s)
    }

    /// Démarre une réunion manuellement.
    pub async fn start_meeting(&self, title: Option<String>) -> Result<Meeting> {
        self.start_meeting_with(title, false).await
    }

    /// `in_person` : réunion en présentiel, sans app de visio (micro du Mac seul).
    pub async fn start_meeting_with(&self, title: Option<String>, in_person: bool) -> Result<Meeting> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if !self.inner.send_live(LiveCmd::Start { title, in_person, reply: tx }) {
            bail!("la session en direct n'est pas démarrée");
        }
        rx.await.context("session en direct arrêtée")?
    }

    // ------------------------------------------------------------------ préparation

    /// Réunions prévues et événements du calendrier des `days` prochains jours.
    pub async fn upcoming(&self, days: i64) -> Result<crate::planning::Upcoming> {
        crate::planning::upcoming(&self.inner, days).await
    }

    pub fn plan_meeting(&self, title: &str, scheduled_at: &str, event_id: Option<&str>) -> Result<Meeting> {
        crate::planning::plan_meeting(&self.inner, title, scheduled_at, event_id)
    }

    /// Démarre l'écoute d'une réunion prévue, même en avance sur l'heure prévue.
    pub async fn start_planned(&self, id: &str, in_person: bool) -> Result<Meeting> {
        let m = self.inner.db.get_meeting(id)?.context("réunion introuvable")?;
        if m.status != "planned" {
            bail!("cette réunion n'est pas une réunion prévue");
        }
        // L'heure prévue devient maintenant : la réunion est reprise au démarrage.
        self.inner.db.with(|c| c.execute("UPDATE meetings SET started_at=?2 WHERE id=?1", rusqlite::params![id, crate::now_iso()]))?;
        let started = self.start_meeting_with(None, in_person).await?;
        if started.id != id {
            bail!("une autre réunion a été démarrée ({})", started.id);
        }
        Ok(started)
    }

    pub fn participants(&self, id: &str) -> Result<Vec<crate::planning::Participant>> {
        self.inner.db.get_meeting(id)?.context("réunion introuvable")?;
        self.inner.db.participants(id)
    }

    pub fn set_participants(&self, id: &str, list: &[crate::planning::Participant]) -> Result<Vec<crate::planning::Participant>> {
        self.inner.db.get_meeting(id)?.context("réunion introuvable")?;
        self.inner.db.set_participants(id, list)?;
        crate::report::context_touched(&self.inner, id)?;
        self.inner.db.participants(id)
    }

    pub async fn prepare_questions(&self, id: &str) -> Result<Vec<String>> {
        crate::planning::prepare_questions(&self.inner, id).await
    }

    pub async fn prepare_apply(&self, id: &str, answers: &[crate::planning::PrepAnswer]) -> Result<crate::context::MeetingContext> {
        crate::planning::prepare_apply(&self.inner, id, answers).await?;
        self.meeting_context(id)
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

    /// Met l'écoute en pause (rien n'est transcrit, la réunion continue) ou la reprend.
    pub fn pause_meeting(&self, on: bool) -> Result<()> {
        if self.live_state().meeting.is_none() {
            bail!("aucune écoute en cours");
        }
        self.inner.send_live(LiveCmd::Pause(on));
        Ok(())
    }

    /// Coupe ou rétablit le micro dans Sténo (indépendant du micro de l'app de visio).
    pub fn set_mic_muted(&self, on: bool) {
        self.inner.send_live(LiveCmd::Mute(on));
    }

    pub fn set_live_confidential(&self, on: bool) -> Result<()> {
        if let Some(m) = self.inner.db.live_meeting()? {
            self.set_confidential(&m.id, on)?;
        } else {
            self.inner.send_live(LiveCmd::NextConfidential(on));
        }
        Ok(())
    }

    /// Micros et sorties disponibles (dernière liste reçue du sidecar).
    pub fn audio_devices(&self) -> crate::sidecar::AudioDevices {
        self.inner.audio_devices.lock().unwrap().clone()
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
    /// `Stop` or `Cancel` when the text starts with such a command verb.
    pub fn control_intent(&self, query: &str) -> Option<Intent> {
        crate::wake::Detector::new(&self.settings().voice)
            .detect_aside(query.trim())
            .filter(|d| d.verb.is_some() && matches!(d.intent, Intent::Stop | Intent::Cancel))
            .map(|d| d.intent)
    }

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
        // « arrête l'écoute », « annule » are commands, not questions about past meetings.
        if self.control_intent(query).is_some() {
            return self.ask(query, None, origin);
        }
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

    /// Retire une phrase qui n'a pas été dite (hallucination de la transcription).
    pub fn delete_segment(&self, id: i64) -> Result<()> {
        let Some(meeting_id) = self.inner.db.delete_segment(id)? else {
            bail!("phrase introuvable");
        };
        self.inner.emit(CoreEvent::SegmentDeleted { meeting_id, id });
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

    // ---------- Contexte et documents ----------

    pub fn meeting_context(&self, id: &str) -> Result<crate::context::MeetingContext> {
        let m = self.inner.db.get_meeting(id)?.context("réunion introuvable")?;
        let mut c = self.inner.db.meeting_context(id)?;
        c.report_at = crate::report::report_at(&self.inner, &m)?;
        if c.template.is_none() {
            c.inherited_template = crate::report::inherited_template(&self.inner, &m)?;
        }
        Ok(c)
    }

    /// Texte libre et modèle de compte rendu (`None` = modèle par défaut des réglages).
    pub fn set_meeting_context(&self, id: &str, text: &str, template: Option<&str>) -> Result<crate::context::MeetingContext> {
        self.inner.db.get_meeting(id)?.context("réunion introuvable")?;
        let template = template.map(str::trim).filter(|t| !t.is_empty());
        if let Some(t) = template {
            crate::report::template(&self.settings(), t).with_context(|| format!("modèle de compte rendu inconnu : {t}"))?;
        }
        let (text, _) = crate::context::cap(text.trim(), crate::context::MAX_FILE_CHARS);
        self.inner.db.set_meeting_context(id, &text, template)?;
        crate::report::context_touched(&self.inner, id)?;
        self.meeting_context(id)
    }

    /// Joint un document (contenu en base64) : son texte est extrait et gardé en base.
    pub fn add_context_file(&self, id: &str, name: &str, data_base64: &str) -> Result<crate::context::ContextFile> {
        use base64::Engine;
        self.inner.db.get_meeting(id)?.context("réunion introuvable")?;
        let name = std::path::Path::new(name.trim())
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .context("nom de fichier manquant")?
            .to_string();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_base64.trim())
            .context("contenu du fichier illisible (base64 attendu)")?;
        let (kind, text, truncated) = crate::context::extract(&name, &bytes)?;
        let fid = self.inner.db.add_context_file(id, &name, kind, bytes.len() as i64, &text, truncated)?;
        crate::report::context_touched(&self.inner, id)?;
        self.inner.db.meeting_context(id)?.files.into_iter().find(|f| f.id == fid).context("document introuvable")
    }

    pub fn delete_context_file(&self, file_id: i64) -> Result<()> {
        if let Some(mid) = self.inner.db.delete_context_file(file_id)? {
            crate::report::context_touched(&self.inner, &mid)?;
        }
        Ok(())
    }

    /// « Rédiger maintenant » : le compte rendu part sans attendre la fin du délai de contexte.
    pub fn report_now(&self, id: &str) -> Result<()> {
        self.inner.db.get_meeting(id)?.context("réunion introuvable")?;
        if self.inner.db.set_job_start("report", id, None)? == 0 {
            self.inner.report_now.lock().unwrap().insert(id.to_string());
        }
        self.inner.jobs_notify.notify_one();
        self.inner.emit(CoreEvent::MeetingsChanged);
        Ok(())
    }

    /// Modèles de compte rendu (intégrés puis perso).
    /// Skill « Sténo » et serveur MCP dans les agents de l'utilisateur.
    pub async fn steno_skill_status(&self) -> agents::skill::SkillStatus {
        agents::skill::status().await
    }

    pub async fn install_steno_skill(&self) -> Result<agents::skill::SkillStatus> {
        agents::skill::install().await
    }

    /// Propose un format de compte rendu à partir d'un exemple collé (non enregistré).
    pub async fn template_from_example(&self, example: &str) -> Result<crate::report::TemplateDraft> {
        crate::report::template_from_example(&self.inner, example).await
    }

    pub fn report_templates(&self) -> Vec<crate::report::Template> {
        crate::report::templates(&self.settings())
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

    /// Export au format demandé (`markdown`, `txt`, `srt`, `vtt`).
    pub fn export_meeting_as(&self, id: &str, format: &str) -> Result<String> {
        crate::export::meeting_export(&self.inner.db, id, format)
    }

    /// Nom de fichier proposé pour un export.
    pub fn export_file_name(&self, id: &str, format: &str) -> Result<String> {
        crate::export::file_name(&self.inner.db, id, format)
    }

    /// Écrit l'export dans `path` (choisi par l'utilisateur dans la fenêtre d'enregistrement).
    pub fn write_export(&self, id: &str, format: &str, path: &std::path::Path) -> Result<()> {
        std::fs::write(path, self.export_meeting_as(id, format)?).with_context(|| format!("écriture de {}", path.display()))
    }

    pub fn add_note(&self, meeting_id: &str, text: &str) -> Result<Note> {
        self.inner.db.add_note(meeting_id, text.trim(), "user")
    }

    // ------------------------------------------------------------------ agents

    pub async fn detect_agents(&self) -> Vec<agents::AgentInfo> {
        agents::detect_all(&self.settings().agent.custom_command).await
    }

    /// Modèles proposés par un agent (liste indicative ; la saisie libre reste possible).
    pub async fn list_models(&self, provider: &str) -> Result<Vec<agents::ModelInfo>> {
        let s = self.settings();
        let adapter = agents::adapter_for(provider, &s.agent.custom_command).context("agent inconnu")?;
        adapter.list_models().await
    }

    /// Envoie une invite de test à un agent (modèle et niveau de réflexion facultatifs).
    pub async fn test_agent(&self, provider: &str) -> Result<(bool, String, u128)> {
        self.test_agent_with(provider, None, None).await
    }

    pub async fn test_agent_with(&self, provider: &str, model: Option<String>, effort: Option<String>) -> Result<(bool, String, u128)> {
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
                    model: model.filter(|m| !m.trim().is_empty()),
                    effort: effort.map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty()),
                    session: SessionMode::None,
                    repos: vec![],
                    exclude_globs: vec![],
                    tools: ToolAccess::None,
                    max_tool_calls: 0,
                    timeout: std::time::Duration::from_secs(120),
                    allow_api_key: s.agent.allow_api_key,
                    run_id: "test".into(),
                    hook_exe: self.inner.hook_exe.clone(),
                    skills: false,
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
        // Another process (the app) owns the live session and the running agents: the request
        // stays queued and a `control` job hands it over (see `jobs::run_control`).
        Intent::Cancel | Intent::Stop if inner.db.other_process_running()? => {}
        Intent::Cancel | Intent::Stop => {
            if let Some((summary, answer)) = run_control(inner, intent, meeting_id)? {
                finish(&mut r, "done", summary, answer);
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
        // L'interface affiche le badge « Demande » sur ces phrases.
        for id in segment_ids {
            if let Some(seg) = inner.db.get_segment(*id)? {
                inner.emit(CoreEvent::Segment(seg));
            }
        }
    }
    inner.emit(CoreEvent::Request(r.clone()));
    if r.status == "queued" && matches!(intent, Intent::Cancel | Intent::Stop) {
        // Not an AI job: never waits behind `agent.max_parallel` (the search it cancels).
        inner.db.enqueue_job("control", r.meeting_id.as_deref(), &serde_json::json!({ "request_id": r.id }))?;
    } else if r.status == "queued" {
        inner.db.enqueue_job("request", r.meeting_id.as_deref(), &serde_json::json!({ "request_id": r.id }))?;
        inner.jobs_notify.notify_one();
        inner.emit(CoreEvent::Notify {
            title: inner.tr("🔎 Je cherche…", "🔎 Searching…"),
            body: crate::text::truncate(query, 120),
            route: None,
        });
    } else if let Some(sum) = &r.summary {
        inner.emit(CoreEvent::Notify { title: "Sténo".into(), body: sum.clone(), route: None });
    }
    Ok(r)
}

/// Runs « annule » (the last active request) or « arrête l'écoute » in this process.
/// Returns the request's summary and answer; `None` for any other intent.
pub(crate) fn run_control(inner: &Inner, intent: Intent, meeting_id: Option<&str>) -> Result<Option<(String, String)>> {
    Ok(Some(match intent {
        Intent::Cancel => match inner.db.last_active_request(meeting_id)? {
            Some(last) => {
                cancel_request(inner, &last.id)?;
                (format!("Demande annulée : {}", crate::text::truncate(&last.query, 80)), "Demande annulée.".into())
            }
            None => ("Aucune demande en cours.".into(), "Aucune demande en cours.".into()),
        },
        Intent::Stop => match inner.db.live_meeting()? {
            Some(_) => {
                inner.send_live(LiveCmd::StopRequested);
                ("⏹ Écoute arrêtée.".into(), "Écoute arrêtée. Le compte rendu est en préparation.".into())
            }
            None => ("Aucune écoute en cours.".into(), "Aucune écoute en cours.".into()),
        },
        _ => return Ok(None),
    }))
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

    /// `steno serve` started while the app records: the meeting still marked live is not
    /// closed under the app's feet; after a crash (no other process), it is.
    #[tokio::test]
    async fn live_meeting_closed_only_without_another_process() {
        let core = Core::new(Db::open_in_memory().unwrap(), Default::default()).unwrap();
        core.db()
            .insert_meeting(&Meeting {
                id: "m1".into(),
                title: Some("Point Nova".into()),
                title_source: Some("user".into()),
                started_at: now_iso(),
                ended_at: None,
                status: "live".into(),
                confidential: false,
                agent_provider: None,
                agent_session_id: None,
                correlation: None,
                call_app: None,
            })
            .unwrap();
        let wait = || tokio::time::sleep(std::time::Duration::from_millis(300));
        let h = crate::live::spawn(core.inner.clone(), false);
        wait().await;
        assert!(core.db().live_meeting().unwrap().is_some());
        h.send(LiveCmd::Shutdown);
        let h = crate::live::spawn(core.inner.clone(), true);
        wait().await;
        assert!(core.db().live_meeting().unwrap().is_none());
        h.send(LiveCmd::Shutdown);
    }

    /// `steno ask "annule"` / `"arrête l'écoute"` while the app runs: the CLI queues the
    /// command and the app runs it, where the agents' cancellation tokens and the live
    /// session are.
    #[tokio::test]
    async fn control_commands_from_another_process_are_forwarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("steno.db");
        let app = Core::new(Db::open(&path).unwrap(), Default::default()).unwrap();
        let cli = Core::new(Db::open(&path).unwrap(), Default::default()).unwrap();
        // Without the app, the CLI runs the command itself.
        let r = cli.ask_history("arrête l'écoute", "cli").unwrap();
        assert_eq!((r.intent.as_str(), r.status.as_str()), ("stop", "done"));
        // A search the app is running.
        let search = create_request(&app.inner, None, "ui", Intent::Search, "Où en est Nova ?", None, &[]).unwrap();
        let token = CancellationToken::new();
        app.inner.cancels.lock().unwrap().insert(search.id.clone(), token.clone());
        app.db().set_runtime("app_heartbeat", "999999999").unwrap();
        let cancel = cli.ask_history("annule", "cli").unwrap();
        assert_eq!((cancel.intent.as_str(), cancel.status.as_str()), ("cancel", "queued"), "handed over to the app");
        assert!(!token.is_cancelled());
        let job = app.db().claim_job(&["control"]).unwrap().expect("control job");
        crate::jobs::execute(&app.inner, job).await;
        assert!(token.is_cancelled(), "the app cancelled its running agent");
        assert_eq!(app.db().get_request(&search.id).unwrap().unwrap().status, "cancelled");
        let cancel = app.db().get_request(&cancel.id).unwrap().unwrap();
        assert_eq!(cancel.status, "done");
        assert!(cancel.summary.unwrap().contains("Où en est Nova"));
    }

    /// `steno settings import` while the app runs: the app picks the new settings up instead
    /// of overwriting them on its next save.
    #[tokio::test]
    async fn settings_saved_by_another_process_are_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("steno.db");
        let app = Core::new(Db::open(&path).unwrap(), Default::default()).unwrap();
        let cli = Core::new(Db::open(&path).unwrap(), Default::default()).unwrap();
        assert!(!app.inner.reload_settings_if_changed().unwrap());
        let mut rx = app.subscribe();
        let mut s = cli.settings();
        s.voice.wake_word = "Greffier".into();
        cli.save_settings(s).unwrap();
        assert!(app.inner.reload_settings_if_changed().unwrap());
        assert_eq!(app.settings().voice.wake_word, "Greffier");
        assert!(matches!(rx.try_recv(), Ok(CoreEvent::SettingsChanged(_))));
        assert!(!app.inner.reload_settings_if_changed().unwrap(), "only once");
        assert!(!cli.inner.reload_settings_if_changed().unwrap(), "its own save");
    }

    #[test]
    fn refus_de_publication() {
        assert!(is_publish_request("Poste ça sur Slack"));
        assert!(is_publish_request("crée le ticket pour ce bug"));
        assert!(is_publish_request("envoie le récap par mail à Paul"));
        assert!(!is_publish_request("rédige un brouillon de message Slack"));
        assert!(!is_publish_request("qui a créé le ticket APP-1234 ?"));
        assert!(!is_publish_request("les erreurs de ms-booking depuis hier"));
    }

    #[test]
    fn refuses_english_publish_requests() {
        assert!(is_publish_request("Post this on Slack"));
        assert!(is_publish_request("Send an email to the team"));
        assert!(is_publish_request("please create a ticket for this bug"));
        assert!(is_publish_request("Can you please share the recap with the team?"));
        assert!(is_publish_request("could you close APP-1234"));
        assert!(is_publish_request("the recap, by email to Paul"));
        assert!(is_publish_request("merge the fix branch"));
        assert!(!is_publish_request("draft a Slack message for the team"));
        assert!(!is_publish_request("who created ticket APP-1234?"));
        assert!(!is_publish_request("open issues on the booking service"));
        assert!(!is_publish_request("merge requests opened since yesterday"));
        assert!(!is_publish_request("post mortem of last week's outage"));
        assert!(!is_publish_request("what did Paul post on Slack yesterday?"));
        // French words that look like English verbs stay allowed.
        assert!(!is_publish_request("comment marche le déploiement ?"));
    }
}
