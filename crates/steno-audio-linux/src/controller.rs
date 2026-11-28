//! Contrôleur du mode protocole : reçoit les commandes, pilote la session de capture, relaie la
//! surveillance des apps de visio (`watch` → `call`), les permissions, les périphériques
//! (`devices`, `set_input`) et le signal sonore (`play`). Port de `Controller.swift`.
//!
//! Une seule boucle (thread principal) traite dans l'ordre : lignes de stdin, événements du
//! serveur son, événements du moniteur d'appels, minuterie. Les travaux longs (test du mot
//! d'appel, diarisation, retranscription, calendrier) partent dans des threads.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{select, unbounded, Receiver};

use crate::calls::{resolve_capture, CallMonitor, CallSnapshot, MonitorEvent};
use crate::capture::{Session, SessionConfig, TapTargets};
use crate::log;
use crate::models::{self, SpeechModels};
use crate::protocol::{self, selection_label, AudioDeviceList, CaptureState, Command, ErrorCode, Outgoing, PermissionValue, PermissionsSnapshot, StartOptions};
use crate::pulse::{DeviceSnapshot, Pulse, PulseEvent, RecordSource};

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Fin d'un arrêt de capture (les WAV ne sont complets qu'à ce moment).
#[derive(Default)]
struct StopSignal {
    done: Mutex<bool>,
    cv: Condvar,
}

impl StopSignal {
    fn set(&self) {
        *self.done.lock().unwrap() = true;
        self.cv.notify_all();
    }
    fn wait(&self, timeout: Option<Duration>) -> bool {
        let g = self.done.lock().unwrap();
        match timeout {
            Some(t) => *self.cv.wait_timeout_while(g, t, |d| !*d).unwrap().0,
            None => *self.cv.wait_while(g, |d| !*d).unwrap(),
        }
    }
}

pub struct Controller {
    pulse: Pulse,
    pulse_events: Receiver<PulseEvent>,
    monitor: CallMonitor,
    monitor_events: Receiver<MonitorEvent>,
    models: SpeechModels,
    session: Option<Arc<Session>>,
    session_active: Arc<AtomicBool>,
    pending_stop: Option<Arc<StopSignal>>,
    mic_muted: bool,
    input_selection: Option<String>,
    capture_request: Vec<String>,
    devices: DeviceSnapshot,
    last_devices: Option<AudioDeviceList>,
    devices_due: Option<Instant>,
    snapshot: CallSnapshot,
    connected: bool,
    shutting_down: bool,
}

impl Controller {
    pub fn new() -> Controller {
        let (pulse, pulse_events) = Pulse::start();
        let (mtx, monitor_events) = unbounded();
        let monitor = CallMonitor::start(pulse.clone(), mtx);
        Controller {
            pulse,
            pulse_events,
            monitor,
            monitor_events,
            models: SpeechModels::default(),
            session: None,
            session_active: Arc::new(AtomicBool::new(false)),
            pending_stop: None,
            mic_muted: false,
            input_selection: None,
            capture_request: vec![],
            devices: DeviceSnapshot::default(),
            last_devices: None,
            devices_due: None,
            snapshot: CallSnapshot::default(),
            connected: false,
            shutting_down: false,
        }
    }

    fn permissions(&self) -> PermissionsSnapshot {
        // Pas d'autorisation à accorder sous Linux : micro et son des apps sont accessibles dès
        // que le serveur son répond. Le titre de l'appel passe par X11 (XWayland sous Wayland).
        let audio = if self.connected { PermissionValue::Granted } else { PermissionValue::Unknown };
        let x11 = std::env::var_os("DISPLAY").is_some();
        PermissionsSnapshot { microphone: audio, system_audio: audio, accessibility: if x11 { PermissionValue::Granted } else { PermissionValue::Unknown }, calendar: crate::calendar::status() }
    }

    fn refresh_devices(&mut self) -> bool {
        match self.pulse.devices() {
            Ok(d) => {
                self.devices = d;
                true
            }
            Err(e) => {
                log::debug(&format!("périphériques : {e:#}"));
                false
            }
        }
    }

    fn emit_devices(&mut self, force: bool) {
        let list = AudioDeviceList { inputs: self.devices.inputs.clone(), outputs: self.devices.outputs.clone() };
        if force || self.last_devices.as_ref() != Some(&list) {
            log::send(&Outgoing::Devices(list.clone()));
            self.last_devices = Some(list);
        }
    }

    /// Démarrage : `ready`, `permissions`, `devices`, surveillance des apps.
    fn boot(&mut self) {
        // Attend la connexion au serveur son (au plus 3 s) pour annoncer un état juste.
        let deadline = Instant::now() + Duration::from_secs(3);
        while !self.connected && Instant::now() < deadline {
            if let Ok(PulseEvent::Connected) = self.pulse_events.recv_timeout(Duration::from_millis(100)) {
                self.connected = true;
            }
        }
        log::send(&Outgoing::Ready { version: protocol::VERSION.into(), asr: models::ASR_NAME.into() });
        log::send(&Outgoing::Permissions(self.permissions()));
        if self.connected {
            self.refresh_devices();
        } else {
            log::warn("serveur son injoignable au démarrage (PulseAudio ou PipeWire avec pipewire-pulse) : nouvel essai en arrière-plan");
        }
        self.emit_devices(true);
        if std::env::var("STENO_PRELOAD_MODELS").as_deref() == Ok("1") {
            self.models.preload();
        }
        log::info(&format!("prêt (steno-audio {}, ASR {}, modèles dans {})", protocol::VERSION, models::ASR_NAME, models::models_dir().display()));
    }

    pub fn run(mut self, lines: Receiver<Option<String>>) -> ! {
        self.boot();
        let ticker = crossbeam_channel::tick(Duration::from_millis(200));
        loop {
            if SHUTDOWN.load(Ordering::SeqCst) {
                self.shutdown("signal");
            }
            select! {
                recv(lines) -> l => match l {
                    Ok(Some(line)) => self.handle_line(&line),
                    Ok(None) | Err(_) => self.shutdown("fin de stdin"),
                },
                recv(self.pulse_events) -> e => if let Ok(e) = e { self.pulse_event(e) },
                recv(self.monitor_events) -> e => if let Ok(e) = e { self.monitor_event(e) },
                recv(ticker) -> _ => self.tick(),
            }
        }
    }

    fn tick(&mut self) {
        if let Some(t) = self.devices_due {
            if Instant::now() >= t {
                self.devices_due = None;
                if self.refresh_devices() {
                    self.emit_devices(false);
                    if let Some(s) = &self.session {
                        s.devices_changed(&self.devices);
                    }
                }
            }
        }
        if let Some(s) = self.session.clone() {
            let pulse = self.pulse.clone();
            s.tick(|| pulse.devices().ok());
        }
    }

    fn pulse_event(&mut self, e: PulseEvent) {
        match e {
            PulseEvent::Connected => {
                self.connected = true;
                self.refresh_devices();
                self.emit_devices(false);
                log::send(&Outgoing::Permissions(self.permissions()));
                // Serveur redémarré pendant une écoute : micro et flux des apps sont rouverts.
                if let Some(s) = &self.session {
                    s.reconnected(&self.devices);
                }
                self.monitor.poke();
            }
            PulseEvent::Disconnected => {
                if self.connected {
                    self.connected = false;
                    log::send(&Outgoing::Permissions(self.permissions()));
                }
            }
            PulseEvent::DevicesChanged => {
                // Anti-rebond : un branchement produit une rafale d'événements.
                self.devices_due.get_or_insert(Instant::now() + Duration::from_millis(300));
            }
            PulseEvent::ClientsChanged => self.monitor.poke(),
            PulseEvent::StreamEnded(id) => {
                if let Some(s) = self.session.clone() {
                    self.refresh_devices();
                    s.stream_ended(id, &self.devices);
                }
            }
        }
    }

    fn targets(&self, snapshot: &CallSnapshot) -> TapTargets {
        let (apps, _) = resolve_capture(&self.capture_request, &snapshot.watched());
        let ids: Vec<String> = apps.iter().map(|a| a.id.clone()).collect();
        TapTargets { app_names: apps.iter().map(|a| a.name.clone()).collect(), streams: snapshot.streams(&ids) }
    }

    fn monitor_event(&mut self, e: MonitorEvent) {
        match e {
            MonitorEvent::Call { app, active, title } => log::send(&Outgoing::Call { active, app: app.id, title }),
            MonitorEvent::Snapshot(snap) => {
                if let Some(s) = self.session.clone() {
                    let t = self.targets(&snap);
                    if snap.apps.iter().any(|a| !a.streams.is_empty()) || !t.streams.is_empty() {
                        self.refresh_devices();
                    }
                    s.update_targets(t, &self.devices);
                }
                self.snapshot = snap;
            }
        }
    }

    fn handle_line(&mut self, line: &str) {
        match protocol::parse(line) {
            Ok(cmd) => self.handle(cmd),
            Err(e) => {
                log::warn(&format!("commande rejetée : {}", e.message()));
                log::send(&Outgoing::Error { code: ErrorCode::BadCommand, message: e.message(), recoverable: true });
            }
        }
    }

    fn handle(&mut self, cmd: Command) {
        if self.shutting_down {
            return;
        }
        match cmd {
            Command::Watch(apps) => {
                log::info(&format!("apps surveillées : {}", if apps.is_empty() { "aucune".into() } else { apps.iter().map(|a| a.id.as_str()).collect::<Vec<_>>().join(", ") }));
                self.monitor.set_apps(apps);
            }
            Command::Start(o) => self.start(o),
            Command::SetInput(sel) => {
                self.input_selection = sel.clone();
                match self.session.clone() {
                    Some(s) => {
                        log::info(&format!("set_input : micro {}", selection_label(&sel)));
                        self.refresh_devices();
                        s.set_input(sel, &self.devices);
                    }
                    None => log::info(&format!("set_input hors capture : micro {} mémorisé pour le prochain start", selection_label(&sel))),
                }
            }
            Command::Devices => {
                self.refresh_devices();
                self.emit_devices(true);
            }
            Command::Play { device, .. } => {
                // Sortie introuvable : sortie par défaut.
                let device = device.filter(|d| self.devices.outputs.iter().any(|o| &o.id == d));
                self.pulse.play(device, crate::devices::detect_tone(), crate::devices::TONE_RATE);
            }
            Command::Stop => self.stop(),
            Command::Aside(on) => {
                if let Some(s) = &self.session {
                    s.set_aside(on);
                }
                log::info(&format!("aparté {}", if on { "activé" } else { "désactivé" }));
            }
            Command::Pause => {
                if let Some(s) = &self.session {
                    s.set_paused(true);
                    log::info("écoute en pause");
                    log::send(&Outgoing::State(CaptureState::Paused));
                }
            }
            Command::Resume => {
                if let Some(s) = &self.session {
                    s.set_paused(false);
                    log::info("écoute reprise");
                    log::send(&Outgoing::State(CaptureState::Listening));
                }
            }
            Command::Mute(on) => {
                self.mic_muted = on;
                if let Some(s) = &self.session {
                    s.set_muted(on);
                }
                log::info(&format!("micro {} dans Sténo", if on { "coupé" } else { "rétabli" }));
            }
            Command::WakeTest { seconds } => self.wake_test(seconds),
            Command::Diarize { meeting_id, audio_dir, file } => {
                let pending = self.pending_stop.clone();
                std::thread::spawn(move || diarize(pending, meeting_id, PathBuf::from(audio_dir), file));
            }
            Command::Retranscribe { meeting_id, audio_dir, language, vocabulary, items } => {
                let pending = self.pending_stop.clone();
                std::thread::spawn(move || retranscribe(pending, meeting_id, PathBuf::from(audio_dir), language, vocabulary, items));
            }
            Command::Permissions => log::send(&Outgoing::Permissions(self.permissions())),
            Command::RequestPermission(kind) => {
                // Aucune invite système sous Linux : on renvoie l'état courant.
                log::info(&format!("request_permission {kind} : aucune autorisation à demander sous Linux"));
                log::send(&Outgoing::Permissions(self.permissions()));
            }
            Command::CalendarEvents { from, to } => {
                let start = from.map(|d| d.with_timezone(&chrono::Local)).unwrap_or_else(chrono::Local::now);
                let end = to.map(|d| d.with_timezone(&chrono::Local)).unwrap_or(start + chrono::Duration::days(7));
                std::thread::spawn(move || log::send(&Outgoing::Calendar { events: crate::calendar::events(start, end) }));
            }
            Command::Shutdown => self.shutdown("commande shutdown"),
        }
    }

    fn start(&mut self, o: StartOptions) {
        if let Some(s) = self.session.clone() {
            log::warn("start reçu alors que la capture tourne déjà : ignoré (sauf input_device)");
            if let Some(sel) = o.input_device {
                if sel != self.input_selection {
                    self.input_selection = sel.clone();
                    self.refresh_devices();
                    s.set_input(sel, &self.devices);
                }
            }
            log::send(&Outgoing::State(CaptureState::Listening));
            return;
        }
        self.models.retry_if_failed();
        self.models.preload();
        if let Some(sel) = o.input_device {
            self.input_selection = sel;
        }
        self.capture_request = o.apps.clone();
        // Relevé frais : le dernier relevé du minuteur peut dater de 2 s ou précéder un `watch`.
        let snapshot = self.monitor.scan_now();
        let (apps, unknown) = resolve_capture(&self.capture_request, &snapshot.watched());
        if !unknown.is_empty() {
            log::warn(&format!("start.apps : app(s) non surveillée(s) ignorée(s) : {}", unknown.join(", ")));
        }
        let targets = self.targets(&snapshot);
        self.snapshot = snapshot;
        self.refresh_devices();
        let cfg = SessionConfig {
            vocabulary: o.vocabulary.clone(),
            silence_ms: o.silence_ms,
            keep_audio: o.keep_audio,
            audio_dir: o.audio_dir.as_ref().map(PathBuf::from),
            input_device: self.input_selection.clone(),
            room: o.room,
        };
        let session = Session::start(cfg, self.pulse.clone(), self.models.clone(), self.mic_muted, &self.devices, targets);
        self.session = Some(Arc::new(session));
        self.session_active.store(true, Ordering::SeqCst);
        log::info(&format!(
            "capture démarrée (micro {}, {}, silence {} ms, vocabulaire {} mots, audio {})",
            selection_label(&self.input_selection),
            if o.room { "présentiel".to_string() } else { format!("apps {}", apps.iter().map(|a| a.id.as_str()).collect::<Vec<_>>().join(", ")) },
            o.silence_ms,
            o.vocabulary.len(),
            if o.keep_audio { "conservé" } else { "non conservé" }
        ));
        log::send(&Outgoing::State(CaptureState::Listening));
    }

    fn stop_session(&mut self) -> Option<Arc<StopSignal>> {
        let s = self.session.take()?;
        self.session_active.store(false, Ordering::SeqCst);
        let done = Arc::new(StopSignal::default());
        self.pending_stop = Some(done.clone());
        let d = done.clone();
        std::thread::spawn(move || {
            // Seul le contrôleur détient la session (les tests du mot d'appel n'ont qu'une référence faible).
            let mut s = s;
            let session = loop {
                match Arc::try_unwrap(s) {
                    Ok(x) => break x,
                    Err(back) => {
                        s = back;
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
            };
            session.stop();
            d.set();
        });
        Some(done)
    }

    fn stop(&mut self) {
        let Some(done) = self.stop_session() else {
            log::send(&Outgoing::State(CaptureState::Stopped));
            return;
        };
        // Vide les énoncés en cours ; au-delà de 15 s, l'arrêt est annoncé quand même et les
        // derniers segments arriveront ensuite.
        if done.wait(Some(Duration::from_secs(15))) {
            log::info("capture arrêtée");
        } else {
            log::warn("vidage des segments plus long que prévu : arrêt annoncé avant la fin");
        }
        log::send(&Outgoing::State(CaptureState::Stopped));
        // Retour au niveau de repos, jamais pendant que les derniers énoncés sont transcrits.
        let (models, active) = (self.models.clone(), self.session_active.clone());
        std::thread::spawn(move || {
            done.wait(None);
            if !active.load(Ordering::SeqCst) {
                models.unload();
            }
        });
    }

    fn wake_test(&mut self, seconds: f64) {
        let models = self.models.clone();
        let dur = Duration::from_secs_f64(seconds);
        if let Some(s) = &self.session {
            let id = s.collector_start();
            let weak = Arc::downgrade(s);
            std::thread::spawn(move || {
                std::thread::sleep(dur);
                let samples = weak.upgrade().map(|s| s.collector_take(id)).unwrap_or_default();
                finish_wake_test(&models, samples);
            });
            return;
        }
        let (pulse, device) = (self.pulse.clone(), self.input_selection.clone().filter(|d| self.devices.inputs.iter().any(|i| &i.id == d)));
        std::thread::spawn(move || {
            let buf = Arc::new(Mutex::new(Vec::<f32>::new()));
            let b = buf.clone();
            match pulse.open(RecordSource::Source(device), Box::new(move |s| b.lock().unwrap().extend_from_slice(&s))) {
                Ok((id, _)) => {
                    std::thread::sleep(dur);
                    pulse.close(id);
                }
                Err(e) => {
                    log::send(&Outgoing::Error { code: ErrorCode::DeviceChanged, message: format!("Micro indisponible pour le test : {e:#}"), recoverable: true });
                    return;
                }
            }
            let samples = std::mem::take(&mut *buf.lock().unwrap());
            finish_wake_test(&models, samples);
        });
    }

    /// Arrêt propre : capture coupée (segments vidés si possible en 5 s), exit(0).
    fn shutdown(&mut self, reason: &str) -> ! {
        if !self.shutting_down {
            self.shutting_down = true;
            log::info(&format!("arrêt ({reason})"));
            self.monitor.stop();
            if let Some(done) = self.stop_session() {
                done.wait(Some(Duration::from_secs(5)));
                log::send(&Outgoing::State(CaptureState::Stopped));
            }
        }
        std::process::exit(0)
    }
}

fn finish_wake_test(models: &SpeechModels, samples: Vec<f32>) {
    let recognizer = match models.recognizer() {
        Ok(r) => r,
        Err((msg, _)) => {
            log::send(&Outgoing::Error { code: ErrorCode::ModelMissing, message: format!("Modèle de transcription indisponible : {msg}"), recoverable: false });
            return;
        }
    };
    // Transcription brute, sans correction par le vocabulaire (c'est ce qu'on teste).
    match recognizer.transcribe(&samples) {
        Ok(text) => log::send(&Outgoing::WakeTest { text: text.trim().to_string() }),
        Err(e) => log::send(&Outgoing::Error { code: ErrorCode::AsrFailed, message: format!("Transcription du test impossible : {e:#}"), recoverable: true }),
    }
}

fn wait_pending(pending: Option<Arc<StopSignal>>) {
    if let Some(p) = pending {
        p.wait(None);
    }
}

fn diarize(pending: Option<Arc<StopSignal>>, meeting_id: String, dir: PathBuf, file: String) {
    // Le fichier n'est complet qu'une fois la capture arrêtée.
    wait_pending(pending);
    let path = dir.join(&file);
    if !path.is_file() {
        log::send(&Outgoing::Error { code: ErrorCode::BadCommand, message: format!("Diarisation impossible : Fichier audio introuvable : {}", path.display()), recoverable: true });
        return;
    }
    let samples = match crate::wav::load_mono_16k(&path) {
        Ok(s) => s,
        Err(e) => {
            log::send(&Outgoing::Error { code: ErrorCode::BadCommand, message: format!("Diarisation impossible : {e:#}"), recoverable: true });
            return;
        }
    };
    // Personne n'a parlé côté appel (fichier vide ou silencieux) : rien à attribuer.
    if samples.len() < 16_000 || crate::levels::dbfs(&samples) < -70.0 {
        log::info(&format!("diarisation : aucune voix dans {file}"));
        log::send(&Outgoing::Diarization { meeting_id, turns: vec![] });
        return;
    }
    let started = Instant::now();
    match models::diarize(&samples) {
        Ok(raw) => {
            let turns = crate::post::turns(&raw, 0.3);
            let speakers: std::collections::HashSet<&str> = turns.iter().map(|t| t.speaker.as_str()).collect();
            log::info(&format!("diarisation : {} tours, {} locuteurs, {:.1} s", turns.len(), speakers.len(), started.elapsed().as_secs_f64()));
            log::send(&Outgoing::Diarization { meeting_id, turns });
        }
        Err(e) => {
            let msg = format!("{e:#}");
            log::error(&format!("diarisation : {msg}"));
            let code = if msg.contains("téléchargement") || msg.contains("chargement") { ErrorCode::ModelMissing } else { ErrorCode::AsrFailed };
            let prefix = if code == ErrorCode::ModelMissing { "Modèles de diarisation indisponibles" } else { "Diarisation impossible" };
            log::send(&Outgoing::Error { code, message: format!("{prefix} : {msg}"), recoverable: true });
        }
    }
}

fn retranscribe(pending: Option<Arc<StopSignal>>, meeting_id: String, dir: PathBuf, language: String, vocabulary: Vec<String>, items: Vec<protocol::RetranscribeItem>) {
    wait_pending(pending);
    let started = Instant::now();
    let recognizer = match models::FinalRecognizer::load(&language) {
        Ok(r) => r,
        Err(e) => {
            log::error(&format!("modèle de retranscription : {e:#}"));
            log::send(&Outgoing::Error { code: ErrorCode::ModelMissing, message: format!("Retranscription impossible : {e:#}"), recoverable: true });
            return;
        }
    };
    log::info(&format!("modèle de retranscription {} prêt en {:.1} s", recognizer.name, started.elapsed().as_secs_f64()));
    let corrector = crate::filters::VocabularyCorrector::new(&vocabulary);
    let out = crate::post::run(&items, &dir, |s| recognizer.transcribe(s), |t| corrector.correct(t), crate::wav::load_mono_16k);
    log::send(&Outgoing::Retranscription { meeting_id, items: out });
}
