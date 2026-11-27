//! Session en direct : pilote le sidecar audio, le cycle de vie des réunions, l'écriture
//! des segments, l'anti-écho, le mot d'appel et l'aparté (PLAN §5.1, §6, §7.1).

use crate::core::{create_request, Inner};
use crate::echo::EchoFilter;
use crate::events::{CoreEvent, Levels};
use crate::model::{Meeting, Segment};
use crate::sidecar::{SidecarCommand, SidecarEvent, SidecarProcess, Turn};
use crate::wake::assembler::{Assembled, CommandAssembler, Utterance};
use crate::wake::Detector;
use crate::{db::NewSegment, now_iso};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

/// Commande de lancement du sidecar.
#[derive(Debug, Clone)]
pub struct SidecarSpec {
    pub program: String,
    pub args: Vec<String>,
}

pub enum LiveCmd {
    Start { title: Option<String>, reply: oneshot::Sender<Result<Meeting>> },
    Stop { reply: oneshot::Sender<Result<()>> },
    Aside(bool),
    NextConfidential(bool),
    /// Arrêt demandé à la voix ou par une demande tapée (« arrête l'écoute »).
    StopRequested,
    SettingsChanged,
    WakeTest { seconds: u32, reply: oneshot::Sender<Result<String>> },
    Diarize { meeting_id: String, reply: oneshot::Sender<Result<Vec<Turn>>> },
    Shutdown,
}

#[derive(Clone)]
pub struct LiveHandle {
    tx: mpsc::UnboundedSender<LiveCmd>,
}

impl LiveHandle {
    pub fn send(&self, cmd: LiveCmd) -> bool {
        self.tx.send(cmd).is_ok()
    }
}

/// Délai avant de clore la commande d'aparté après le relâchement du raccourci.
const ASIDE_GRACE: Duration = Duration::from_millis(2500);

struct Session {
    inner: Arc<Inner>,
    sidecar: Option<SidecarProcess>,
    events: Option<mpsc::UnboundedReceiver<SidecarEvent>>,
    meeting: Option<Meeting>,
    capturing: bool,
    detector: Detector,
    assembler: CommandAssembler,
    echo: EchoFilter,
    echo_ids: HashSet<i64>,
    aside: bool,
    aside_buf: Vec<(i64, String)>,
    aside_flush_at: Option<Instant>,
    end_at: Option<Instant>,
    /// Appels en cours (app, titre), du plus ancien au plus récent.
    calls: Vec<(String, Option<String>)>,
    /// Micro demandé au sidecar (UID, `None` = défaut du système).
    input_device: Option<String>,
    next_confidential: bool,
    wake_waiters: Vec<oneshot::Sender<Result<String>>>,
    diarize_waiters: HashMap<String, oneshot::Sender<Result<Vec<Turn>>>>,
    restart_at: Option<Instant>,
    backoff: Duration,
    t_offset: f64,
    t_max: f64,
    levels: Levels,
    last_level_emit: Instant,
}

pub fn spawn(inner: Arc<Inner>) -> LiveHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let detector = Detector::new(&inner.settings().voice);
    let mut s = Session {
        inner,
        sidecar: None,
        events: None,
        meeting: None,
        capturing: false,
        detector,
        assembler: CommandAssembler::new(),
        echo: EchoFilter::new(),
        echo_ids: HashSet::new(),
        aside: false,
        aside_buf: vec![],
        aside_flush_at: None,
        end_at: None,
        calls: vec![],
        input_device: None,
        next_confidential: false,
        wake_waiters: vec![],
        diarize_waiters: HashMap::new(),
        restart_at: None,
        backoff: Duration::from_secs(2),
        t_offset: 0.0,
        t_max: 0.0,
        levels: Levels { me: -90.0, them: -90.0 },
        last_level_emit: Instant::now(),
    };
    tokio::spawn(async move { s.run(rx).await });
    LiveHandle { tx }
}

async fn recv_opt(rx: &mut Option<mpsc::UnboundedReceiver<SidecarEvent>>) -> Option<SidecarEvent> {
    match rx {
        Some(r) => r.recv().await,
        None => std::future::pending().await,
    }
}

impl Session {
    async fn run(&mut self, mut cmds: mpsc::UnboundedReceiver<LiveCmd>) {
        // Une réunion restée « live » (plantage) est close au démarrage.
        if let Ok(Some(m)) = self.inner.db.live_meeting() {
            let _ = self.inner.db.end_meeting(&m.id, &now_iso());
            let _ = self.after_end(&m);
        }
        self.start_sidecar();
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            tokio::select! {
                c = cmds.recv() => match c {
                    Some(LiveCmd::Shutdown) | None => break,
                    Some(c) => self.on_cmd(c).await,
                },
                ev = recv_opt(&mut self.events) => match ev {
                    Some(ev) => self.on_event(ev).await,
                    None => self.on_event(SidecarEvent::Exited { code: None }).await,
                },
                _ = tick.tick() => self.on_tick().await,
            }
        }
        if self.meeting.is_some() {
            let _ = self.end_meeting().await;
        }
        if let Some(sc) = self.sidecar.take() {
            sc.shutdown().await;
        }
    }

    fn start_sidecar(&mut self) {
        let Some(spec) = self.inner.sidecar_spec.clone() else {
            self.inner.set_live_state(|s| s.sidecar = "missing".into());
            return;
        };
        self.inner.set_live_state(|s| s.sidecar = "starting".into());
        match SidecarProcess::spawn(&spec.program, &spec.args) {
            Ok((p, rx)) => {
                self.sidecar = Some(p);
                self.events = Some(rx);
                self.inner.set_live_state(|s| {
                    s.sidecar = "running".into();
                    s.last_error = None;
                });
                self.send_watch();
                self.input_device = self.inner.settings().audio.input_device;
                // Reprise d'une capture interrompue.
                if self.capturing {
                    self.t_offset = self.t_max;
                    self.send_start();
                }
            }
            Err(e) => {
                let missing = !std::path::Path::new(&spec.program).exists() && which::which(&spec.program).is_err();
                self.inner.set_live_state(|s| {
                    s.sidecar = if missing { "missing".into() } else { "error".into() };
                    s.last_error = Some(format!("{e:#}"));
                });
                if !missing {
                    self.restart_at = Some(Instant::now() + self.backoff);
                    self.backoff = (self.backoff * 2).min(Duration::from_secs(60));
                }
            }
        }
    }

    fn send(&self, cmd: SidecarCommand) -> bool {
        match &self.sidecar {
            Some(s) => {
                s.send(cmd);
                true
            }
            None => false,
        }
    }

    fn send_start(&self) {
        let s = self.inner.settings();
        let Some(m) = &self.meeting else { return };
        let mut vocab = vec![s.voice.wake_word.clone()];
        vocab.extend(s.audio.vocabulary.iter().cloned());
        let dir = crate::paths::cache_dir().join(&m.id);
        let _ = std::fs::create_dir_all(&dir);
        // Son capté : celui de l'app de l'appel ; sans appel connu, toutes les apps surveillées.
        let apps = m.call_app.clone().into_iter().collect();
        self.send(SidecarCommand::Start {
            vocabulary: vocab,
            silence_ms: s.audio.silence_ms,
            keep_audio: true,
            audio_dir: dir.display().to_string(),
            apps,
            input_device: s.audio.input_device.clone(),
        });
    }

    /// Commande vocale détectée : signal discret joué par le sidecar sur la sortie choisie
    /// (jamais dans le son de l'appel) ; sans sidecar, l'app peut le jouer elle-même.
    fn signal_command(&self) {
        let s = self.inner.settings();
        let mut played = false;
        if s.voice.feedback != "none" && self.sidecar.is_some() {
            let device = match (s.voice.feedback.as_str(), &s.voice.feedback_device) {
                (_, Some(d)) => Some(Some(d.clone())),
                // « Casque uniquement » sans sortie choisie : la sortie par défaut si c'est un casque.
                ("headset", None) => headset_default(&self.inner.audio_devices.lock().unwrap()).map(Some),
                _ => Some(None),
            };
            if let Some(device) = device {
                self.send(SidecarCommand::Play { sound: "detect".into(), device });
            }
            played = true;
        }
        self.inner.emit(CoreEvent::CommandDetected { played });
    }

    /// Envoie au sidecar la liste des apps de visio à surveiller.
    fn send_watch(&self) {
        self.send(SidecarCommand::Watch { apps: watch_list(&self.inner.settings().audio) });
    }

    fn current_call(&self) -> Option<&(String, Option<String>)> {
        self.calls.last()
    }

    fn publish_call_state(&self) {
        let s = self.inner.settings();
        let st = match self.current_call() {
            Some((app, title)) => crate::events::CallState {
                active: true,
                app: Some(app.clone()),
                app_name: Some(s.audio.call_app(app).map(|a| a.name.clone()).unwrap_or_else(|| app.clone())),
                title: title.clone(),
            },
            None => crate::events::CallState::default(),
        };
        self.inner.set_live_state(|l| l.call = st);
    }

    fn title_excluded(&self, title: Option<&str>) -> bool {
        let Some(t) = title else { return false };
        let s = self.inner.settings();
        let nt = crate::text::normalize(t);
        s.privacy
            .excluded_titles
            .iter()
            .map(|w| crate::text::normalize(w))
            .any(|w| !w.is_empty() && nt.contains(&w))
    }

    fn create_meeting(&mut self, title: Option<String>, source: &str, call_app: Option<String>) -> Result<Meeting> {
        let s = self.inner.settings();
        let now = chrono::Local::now();
        let base = format!(
            "{}-{}",
            now.format("%Y-%m-%d-%H%M"),
            crate::text::slugify(title.as_deref().unwrap_or("reunion")).if_empty("reunion")
        );
        let mut id = base.clone();
        let mut n = 2;
        while self.inner.db.meeting_exists(&id)? {
            id = format!("{base}-{n}");
            n += 1;
        }
        let m = Meeting {
            id,
            title: Some(title.clone().unwrap_or_else(|| format!("Réunion du {}", now.format("%d/%m à %H:%M")))),
            title_source: Some(if title.is_some() { source.to_string() } else { "inferred".into() }),
            started_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
            ended_at: None,
            status: "live".into(),
            // Réunion exclue (Réglages → Confidentialité) : écoute possible, mais aucune IA.
            confidential: std::mem::take(&mut self.next_confidential) || self.title_excluded(title.as_deref()),
            agent_provider: None,
            agent_session_id: None,
            correlation: Some(s.correlation.mode.clone()),
            call_app,
        };
        self.inner.db.insert_meeting(&m)?;
        self.meeting = Some(m.clone());
        self.capturing = true;
        self.assembler = CommandAssembler::new();
        self.echo = EchoFilter::new();
        self.echo_ids.clear();
        self.t_offset = 0.0;
        self.t_max = 0.0;
        self.end_at = None;
        self.send_start();
        let mc = m.clone();
        self.inner.set_live_state(|st| {
            st.state = "listening".into();
            st.meeting = Some(mc);
            st.aside = false;
        });
        self.inner.emit(CoreEvent::MeetingsChanged);
        Ok(m)
    }

    async fn end_meeting(&mut self) -> Result<()> {
        if let Some(a) = self.assembler.flush(&self.detector) {
            self.handle_assembled(a);
        }
        self.flush_aside();
        self.send(SidecarCommand::Stop);
        self.capturing = false;
        self.end_at = None;
        let Some(m) = self.meeting.take() else { return Ok(()) };
        self.inner.db.end_meeting(&m.id, &now_iso())?;
        self.inner.set_live_state(|st| {
            st.state = "idle".into();
            st.meeting = None;
            st.aside = false;
            st.levels = Levels { me: -90.0, them: -90.0 };
        });
        self.aside = false;
        self.after_end(&m)?;
        Ok(())
    }

    /// Après la fin : diarisation puis compte rendu (sauf réunion confidentielle).
    fn after_end(&self, m: &Meeting) -> Result<()> {
        let fresh = self.inner.db.get_meeting(&m.id)?.unwrap_or_else(|| m.clone());
        if fresh.confidential {
            self.inner.db.set_meeting_status(&m.id, "done")?;
        } else {
            let wav = crate::paths::cache_dir().join(&m.id).join("them.wav");
            let kind = if wav.exists() && self.sidecar.is_some() { "diarize" } else { "report" };
            self.inner.db.enqueue_job(kind, Some(&m.id), &serde_json::json!({}))?;
            self.inner.jobs_notify.notify_one();
            self.inner.emit(CoreEvent::Notify {
                title: "Réunion terminée".into(),
                body: "Compte rendu en préparation…".into(),
                route: None,
            });
        }
        self.inner.emit(CoreEvent::MeetingsChanged);
        Ok(())
    }

    async fn on_cmd(&mut self, c: LiveCmd) {
        match c {
            LiveCmd::Start { title, reply } => {
                let r = match &self.meeting {
                    Some(m) => Ok(m.clone()),
                    None => {
                        let given = title.filter(|t| !t.trim().is_empty());
                        let call = self.current_call().cloned();
                        let (t, src) = match (given, &call) {
                            (Some(t), _) => (Some(t), "user"),
                            (None, Some((_, title))) => (title.clone(), "call_window"),
                            (None, None) => (None, "inferred"),
                        };
                        self.create_meeting(t, src, call.map(|(app, _)| app))
                    }
                };
                let _ = reply.send(r);
            }
            LiveCmd::Stop { reply } => {
                let _ = reply.send(self.end_meeting().await);
            }
            LiveCmd::Aside(on) => {
                if on == self.aside {
                    return;
                }
                self.aside = on;
                self.send(SidecarCommand::Aside { on });
                if !on {
                    self.aside_flush_at = Some(Instant::now() + ASIDE_GRACE);
                }
                self.inner.set_live_state(|s| s.aside = on);
            }
            LiveCmd::NextConfidential(on) => self.next_confidential = on,
            LiveCmd::StopRequested => {
                if self.meeting.is_some() {
                    // Laisse le temps à la phrase en cours d'être enregistrée.
                    self.end_at = Some(Instant::now() + Duration::from_millis(300));
                }
            }
            LiveCmd::SettingsChanged => {
                self.detector = Detector::new(&self.inner.settings().voice);
                self.send_watch();
                // Changement de micro : appliqué tout de suite, même pendant l'écoute.
                let wanted = self.inner.settings().audio.input_device;
                if wanted != self.input_device {
                    self.input_device = wanted.clone();
                    self.send(SidecarCommand::SetInput { device: wanted });
                }
                // Une app désactivée ne compte plus comme appel en cours.
                let s = self.inner.settings();
                self.calls.retain(|(app, _)| s.audio.call_app(app).map(|a| a.enabled).unwrap_or(false));
                self.publish_call_state();
            }
            LiveCmd::WakeTest { seconds, reply } => {
                if self.send(SidecarCommand::WakeTest { seconds }) {
                    self.wake_waiters.push(reply);
                } else {
                    let _ = reply.send(Err(anyhow!("la capture audio n'est pas disponible (sidecar absent)")));
                }
            }
            LiveCmd::Diarize { meeting_id, reply } => {
                let dir = crate::paths::cache_dir().join(&meeting_id);
                if self.send(SidecarCommand::Diarize { meeting_id: meeting_id.clone(), audio_dir: dir.display().to_string() }) {
                    self.diarize_waiters.insert(meeting_id, reply);
                } else {
                    let _ = reply.send(Err(anyhow!("sidecar absent")));
                }
            }
            LiveCmd::Shutdown => {}
        }
    }

    async fn on_event(&mut self, ev: SidecarEvent) {
        match ev {
            SidecarEvent::Ready { version, asr } => {
                tracing::info!("sidecar prêt (v{version}, {asr})");
                self.backoff = Duration::from_secs(2);
                self.send_watch();
            }
            SidecarEvent::Permissions { microphone, system_audio, accessibility } => {
                let _ = self.inner.db.set_runtime(
                    "permissions",
                    &serde_json::json!({"microphone": microphone, "system_audio": system_audio, "accessibility": accessibility}).to_string(),
                );
            }
            SidecarEvent::Call { active, app, title } => self.on_call(&app, active, title).await,
            SidecarEvent::TeamsCall { active, title } => self.on_call("teams", active, title).await,
            SidecarEvent::Segment { ts, t0, t1, channel, text, private } => {
                if let Err(e) = self.on_segment(ts, t0, t1, &channel, &text, private) {
                    tracing::error!("segment non enregistré : {e:#}");
                }
            }
            SidecarEvent::Devices { inputs, outputs } => {
                *self.inner.audio_devices.lock().unwrap() = crate::sidecar::AudioDevices { inputs, outputs };
            }
            SidecarEvent::Level { me, them } => {
                self.levels = Levels { me, them };
                if self.last_level_emit.elapsed() >= Duration::from_millis(250) {
                    self.last_level_emit = Instant::now();
                    let l = self.levels.clone();
                    self.inner.set_live_state(|s| s.levels = l);
                }
            }
            SidecarEvent::State { state } => {
                if self.meeting.is_some() {
                    let st = if state == "stopped" { "listening".to_string() } else { state };
                    self.inner.set_live_state(|s| s.state = st);
                }
            }
            SidecarEvent::WakeTest { text } => {
                if !self.wake_waiters.is_empty() {
                    let w = self.wake_waiters.remove(0);
                    let _ = w.send(Ok(text));
                }
            }
            SidecarEvent::Diarization { meeting_id, turns } => {
                let key = if self.diarize_waiters.contains_key(&meeting_id) {
                    Some(meeting_id)
                } else {
                    self.diarize_waiters.keys().next().cloned()
                };
                if let Some(k) = key {
                    if let Some(w) = self.diarize_waiters.remove(&k) {
                        let _ = w.send(Ok(turns));
                    }
                }
            }
            SidecarEvent::Error { code, message, recoverable } => {
                tracing::warn!("sidecar : {code} {message}");
                let msg = format!("{code} : {message}");
                let lower = message.to_lowercase();
                if code == "diarize_failed" || code == "diarization_failed" || lower.contains("diarisation") {
                    for (_, w) in self.diarize_waiters.drain() {
                        let _ = w.send(Err(anyhow!(msg.clone())));
                    }
                }
                if code == "wake_test_failed" || (!self.wake_waiters.is_empty() && lower.contains("test")) {
                    for w in self.wake_waiters.drain(..) {
                        let _ = w.send(Err(anyhow!(msg.clone())));
                    }
                }
                self.inner.set_live_state(|s| {
                    s.last_error = Some(msg.clone());
                    if !recoverable {
                        s.sidecar = "error".into();
                    }
                });
                if code == "device_missing" {
                    self.inner.emit(CoreEvent::Notify {
                        title: "🎙 Micro introuvable".into(),
                        body: message.clone(),
                        route: Some("/settings/audio".into()),
                    });
                }
                if !recoverable && matches!(code.as_str(), "mic_denied" | "system_audio_denied") {
                    self.inner.emit(CoreEvent::Notify {
                        title: "⚠️ Permission manquante".into(),
                        body: message,
                        route: Some("/settings/audio".into()),
                    });
                }
            }
            SidecarEvent::Exited { .. } => {
                self.sidecar = None;
                self.events = None;
                for w in self.wake_waiters.drain(..) {
                    let _ = w.send(Err(anyhow!("le sidecar audio s'est arrêté")));
                }
                for (_, w) in self.diarize_waiters.drain() {
                    let _ = w.send(Err(anyhow!("le sidecar audio s'est arrêté")));
                }
                self.inner.set_live_state(|s| s.sidecar = "error".into());
                self.restart_at = Some(Instant::now() + self.backoff);
                self.backoff = (self.backoff * 2).min(Duration::from_secs(60));
            }
            SidecarEvent::Unknown => {}
        }
    }

    /// Appel détecté ou terminé dans une app de visio. **Jamais** de démarrage ni d'arrêt
    /// automatique de l'écoute : au plus une notification qui propose de démarrer.
    async fn on_call(&mut self, app: &str, active: bool, title: Option<String>) {
        let s = self.inner.settings();
        let Some(conf) = s.audio.call_app(app).filter(|a| a.enabled).cloned() else {
            tracing::debug!("appel ignoré : app « {app} » non surveillée");
            return;
        };
        let title = title.filter(|t| !t.trim().is_empty());
        let was_active = self.calls.iter().any(|(a, _)| a == app);
        if active {
            match self.calls.iter_mut().find(|(a, _)| a == app) {
                Some(c) => {
                    if title.is_some() {
                        c.1 = title.clone();
                    }
                }
                None => self.calls.push((app.to_string(), title.clone())),
            }
        } else {
            self.calls.retain(|(a, _)| a != app);
        }
        self.publish_call_state();
        match &self.meeting {
            Some(m) if active && m.call_app.as_deref() == Some(app) => {
                // Le titre de la fenêtre d'appel remplace un titre déduit (jamais celui saisi).
                if let Some(t) = &title {
                    if m.title_source.as_deref() != Some("user") {
                        let _ = self.inner.db.set_meeting_title_auto(&m.id, t, "call_window");
                        if let Some(mm) = &mut self.meeting {
                            mm.title = Some(t.clone());
                            mm.title_source = Some("call_window".into());
                        }
                        self.inner.emit(CoreEvent::MeetingsChanged);
                    }
                }
            }
            None if active && !was_active && s.audio.auto_detect_calls && !self.title_excluded(title.as_deref()) => {
                let shortcut = s.audio.shortcuts.toggle.clone();
                let who = crate::settings::short_app_name(&conf);
                self.inner.emit(CoreEvent::Notify {
                    title: if conf.browser { "Appel détecté dans le navigateur".into() } else { format!("Appel {who} détecté") },
                    body: format!(
                        "{}Démarrer l'écoute : icône Scribe dans la barre de menus ou {shortcut}.",
                        title.as_deref().map(|t| format!("« {t} ». ")).unwrap_or_default()
                    ),
                    route: Some("/live".into()),
                });
            }
            _ => {}
        }
    }

    fn on_segment(&mut self, ts: String, t0: Option<f64>, t1: Option<f64>, channel: &str, text: &str, private: bool) -> Result<()> {
        let Some(m) = self.meeting.clone() else { return Ok(()) };
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        let t0 = t0.map(|t| t + self.t_offset);
        let t1 = t1.map(|t| t + self.t_offset);
        let (a, b) = (t0.unwrap_or(self.t_max), t1.or(t0).unwrap_or(self.t_max));
        self.t_max = self.t_max.max(b);
        let me = channel == "me";
        let private = me && (private || self.aside);
        let seg = self.inner.db.insert_segment(&NewSegment {
            meeting_id: m.id.clone(),
            ts,
            t0,
            t1,
            channel: if me { "me".into() } else { "them".into() },
            speaker: if me { "Moi".into() } else { "Eux".into() },
            text: text.to_string(),
            private,
            is_echo: false,
        })?;
        let mut seg = seg;
        if me {
            if !private && self.echo.check_me(seg.id, a, b, text) {
                self.inner.db.mark_echo(seg.id)?;
                seg.is_echo = true;
                self.echo_ids.insert(seg.id);
            }
        } else {
            for id in self.echo.add_them(a, b, text) {
                self.inner.db.mark_echo(id)?;
                self.echo_ids.insert(id);
            }
            if let Some(cmd) = self.assembler.observe_them(&self.detector, a) {
                self.handle_assembled(cmd);
            }
        }
        self.inner.emit(CoreEvent::Segment(seg.clone()));
        if me && !seg.is_echo {
            if private {
                // Seuls les apartés déclenchés par le raccourci forment une demande.
                if self.aside || self.aside_flush_at.is_some() {
                    self.aside_buf.push((seg.id, seg.text.clone()));
                }
            } else {
                let u = Utterance { id: seg.id, t0: a, t1: b, text: seg.text.clone() };
                for cmd in self.assembler.push_me(&self.detector, &u, Instant::now()) {
                    self.handle_assembled(cmd);
                }
            }
        }
        Ok(())
    }

    fn handle_assembled(&mut self, a: Assembled) {
        if a.segment_ids.iter().any(|id| self.echo_ids.contains(id)) {
            return;
        }
        let Some(d) = a.detection else { return };
        let Some(m) = &self.meeting else { return };
        self.signal_command();
        if let Err(e) = create_request(&self.inner, Some(&m.id), "voice", d.intent, &d.query, d.verb.as_deref(), &a.segment_ids) {
            tracing::warn!("commande vocale non créée : {e:#}");
        }
    }

    fn flush_aside(&mut self) {
        self.aside_flush_at = None;
        if self.aside_buf.is_empty() {
            return;
        }
        let ids: Vec<i64> = self.aside_buf.iter().map(|(i, _)| *i).collect();
        let text = self.aside_buf.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" ");
        self.aside_buf.clear();
        let Some(m) = &self.meeting else { return };
        if let Some(d) = self.detector.detect_aside(&text) {
            self.signal_command();
            if let Err(e) = create_request(&self.inner, Some(&m.id), "hotkey", d.intent, &d.query, d.verb.as_deref(), &ids) {
                tracing::warn!("demande en aparté non créée : {e:#}");
            }
        }
    }

    async fn on_tick(&mut self) {
        let now = Instant::now();
        if let Some(a) = self.assembler.tick(&self.detector, now) {
            self.handle_assembled(a);
        }
        if self.aside_flush_at.map(|t| now >= t).unwrap_or(false) && !self.aside {
            self.flush_aside();
        }
        if self.end_at.map(|t| now >= t).unwrap_or(false) {
            if let Err(e) = self.end_meeting().await {
                tracing::error!("fin de réunion : {e:#}");
            }
        }
        if self.restart_at.map(|t| now >= t).unwrap_or(false) && self.sidecar.is_none() {
            self.restart_at = None;
            self.start_sidecar();
        }
    }
}

/// Sortie par défaut si c'est un casque ou des écouteurs (Bluetooth, USB).
pub fn headset_default(d: &crate::sidecar::AudioDevices) -> Option<String> {
    d.outputs
        .iter()
        .find(|o| o.default && matches!(o.transport.as_str(), "bluetooth" | "usb"))
        .map(|o| o.id.clone())
}

/// Apps surveillées (activées) au format du protocole.
pub fn watch_list(audio: &crate::settings::AudioSettings) -> Vec<crate::sidecar::WatchApp> {
    audio
        .call_apps
        .iter()
        .filter(|a| a.enabled)
        .map(|a| crate::sidecar::WatchApp { id: a.id.clone(), name: a.name.clone(), bundles: a.bundles.clone() })
        .collect()
}

trait IfEmpty {
    fn if_empty(self, d: &str) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, d: &str) -> String {
        if self.is_empty() {
            d.to_string()
        } else {
            self
        }
    }
}

/// Attribue `Eux-1`, `Eux-2`… aux segments `them` selon les tours de parole.
pub fn assign_speakers(segments: &[Segment], turns: &[Turn]) -> Vec<(i64, String)> {
    let mut labels: HashMap<String, String> = HashMap::new();
    let mut out = vec![];
    for s in segments.iter().filter(|s| s.channel == "them") {
        let (a, b) = (s.t0.unwrap_or(0.0), s.t1.unwrap_or(0.0));
        let best = turns
            .iter()
            .map(|t| (t, (b.min(t.t1) - a.max(t.t0)).max(0.0)))
            .filter(|(_, o)| *o > 0.0)
            .max_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((t, _)) = best {
            let n = labels.len() + 1;
            let label = labels.entry(t.speaker.clone()).or_insert_with(|| format!("Eux-{n}")).clone();
            out.push((s.id, label));
        }
    }
    out
}

/// Diarisation hors ligne du canal `them` (via le sidecar), puis renommage des segments.
pub async fn diarize(inner: &Inner, meeting_id: &str) -> Result<()> {
    let wav = crate::paths::cache_dir().join(meeting_id).join("them.wav");
    if !wav.exists() {
        bail!("pas d'audio conservé pour cette réunion");
    }
    let (tx, rx) = oneshot::channel();
    if !inner.send_live(LiveCmd::Diarize { meeting_id: meeting_id.into(), reply: tx }) {
        bail!("session en direct indisponible");
    }
    let turns = tokio::time::timeout(Duration::from_secs(900), rx)
        .await
        .map_err(|_| anyhow!("diarisation : délai dépassé"))?
        .context("diarisation interrompue")??;
    let segs = inner.db.segments(meeting_id, true, true)?;
    let assigned = assign_speakers(&segs, &turns);
    let distinct: HashSet<&String> = assigned.iter().map(|(_, l)| l).collect();
    if distinct.len() < 2 {
        return Ok(()); // un seul interlocuteur : on garde « Eux »
    }
    for (id, label) in &assigned {
        inner.db.set_segment_speaker(*id, label)?;
    }
    for l in distinct {
        inner.db.upsert_speaker(meeting_id, l, None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casque_par_defaut() {
        use crate::sidecar::{AudioDevice, AudioDevices};
        let dev = |id: &str, default: bool, transport: &str| AudioDevice { id: id.into(), name: id.into(), default, transport: transport.into() };
        let mut d = AudioDevices { inputs: vec![], outputs: vec![dev("hp", true, "builtin"), dev("airpods", false, "bluetooth")] };
        assert_eq!(headset_default(&d), None, "haut-parleurs par défaut : pas de casque");
        d.outputs = vec![dev("hp", false, "builtin"), dev("airpods", true, "bluetooth")];
        assert_eq!(headset_default(&d).as_deref(), Some("airpods"));
    }

    #[test]
    fn attribution_des_intervenants() {
        let seg = |id, t0: f64, t1: f64| Segment {
            id,
            meeting_id: "m".into(),
            seq: id,
            ts: String::new(),
            t0: Some(t0),
            t1: Some(t1),
            channel: "them".into(),
            speaker: "Eux".into(),
            text: "x".into(),
            private: false,
            is_echo: false,
            request_id: None,
        };
        let turns = vec![
            Turn { t0: 0.0, t1: 10.0, speaker: "S7".into() },
            Turn { t0: 10.0, t1: 20.0, speaker: "S2".into() },
            Turn { t0: 20.0, t1: 30.0, speaker: "S7".into() },
        ];
        let a = assign_speakers(&[seg(1, 1.0, 4.0), seg(2, 11.0, 15.0), seg(3, 21.0, 25.0)], &turns);
        assert_eq!(a, vec![(1, "Eux-1".into()), (2, "Eux-2".into()), (3, "Eux-1".into())]);
    }
}
