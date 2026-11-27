//! Session de capture : du `start` au `stop`. Port de `CaptureSession.swift`.
//!
//! Possède le flux du micro (canal « me »), les flux des apps captées (canal « them », un par
//! flux de lecture de l'app, mélangés), les deux chaînes de traitement, les WAV éventuels, la
//! minuterie des niveaux et l'annulation d'écho. Gère la recréation des flux (app qui ouvre un
//! nouveau flux, sortie changée, serveur son redémarré), le changement de micro en cours de
//! capture (`set_input`), le micro débranché (repli sur le micro par défaut, `device_missing`)
//! et le mode aparté.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::aec::EchoCanceller3;
use crate::calls::PlaybackStream;
use crate::levels::LevelMeter;
use crate::log::{self, RateLimiter};
use crate::models::SpeechModels;
use crate::pipeline::{ChannelPipeline, PipelineConfig, PipelineFeed};
use crate::protocol::{iso8601, Channel, ErrorCode, Outgoing, SegmentPayload};
use crate::pulse::{DeviceSnapshot, Pulse, RecordSource};
use crate::wav::WavWriter;

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub vocabulary: Vec<String>,
    pub silence_ms: u32,
    pub keep_audio: bool,
    pub audio_dir: Option<PathBuf>,
    /// Micro (`None` = micro par défaut du système, en suivant ses changements).
    pub input_device: Option<String>,
    /// Réunion en présentiel : micro seul, sans capture des apps ni annulation d'écho.
    pub room: bool,
}

/// Ce qu'il faut capter sur le canal « them ».
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TapTargets {
    pub app_names: Vec<String>,
    pub streams: Vec<PlaybackStream>,
}

impl TapTargets {
    pub fn label(&self) -> String {
        if self.app_names.is_empty() {
            "aucune app".into()
        } else {
            self.app_names.join(", ")
        }
    }
}

/// Plan B : tout le son de la sortie par défaut (`STENO_TAP_MODE=global`).
pub fn global_mode() -> bool {
    std::env::var("STENO_TAP_MODE").as_deref() == Ok("global")
}

// MARK: - Mélangeur des flux d'apps

/// Mélange plusieurs flux d'apps arrivant de façon asynchrone : le flux « principal » cadence la
/// sortie, les autres y sont ajoutés (au plus 0,5 s de retard gardé).
#[derive(Default)]
struct Mixer {
    bufs: HashMap<u64, Vec<f32>>,
    primary: Option<u64>,
}

const MIX_MAX_BACKLOG: usize = 8_000;

impl Mixer {
    fn push(&mut self, id: u64, samples: Vec<f32>) -> Vec<f32> {
        self.bufs.entry(id).or_default().extend_from_slice(&samples);
        let primary = match self.primary {
            Some(p) if self.bufs.contains_key(&p) => p,
            _ => id,
        };
        // Le flux principal s'est tu (en pause) alors qu'un autre parle : il prend le relais.
        let primary = if id != primary && self.bufs[&primary].is_empty() && self.bufs[&id].len() > 3_200 { id } else { primary };
        self.primary = Some(primary);
        if id != primary {
            for b in self.bufs.values_mut() {
                if b.len() > MIX_MAX_BACKLOG {
                    let drop = b.len() - MIX_MAX_BACKLOG;
                    b.drain(..drop);
                }
            }
            return vec![];
        }
        let mut out = std::mem::take(self.bufs.get_mut(&primary).unwrap());
        let n = out.len();
        for (k, b) in self.bufs.iter_mut() {
            if *k == primary {
                continue;
            }
            let take = n.min(b.len());
            for (o, s) in out.iter_mut().zip(b.drain(..take)) {
                *o = (*o + s).clamp(-1.0, 1.0);
            }
            if b.len() > MIX_MAX_BACKLOG {
                let drop = b.len() - MIX_MAX_BACKLOG;
                b.drain(..drop);
            }
        }
        out
    }

    fn remove(&mut self, id: u64) {
        self.bufs.remove(&id);
        if self.primary == Some(id) {
            self.primary = None;
        }
    }
}

// MARK: - Session

#[derive(Default)]
struct Flags {
    aside: Vec<(f64, Option<f64>)>,
    muted: bool,
    paused: bool,
    collectors: HashMap<u64, Vec<f32>>,
    next_collector: u64,
}

struct Shared {
    flags: Arc<Mutex<Flags>>,
    aec: Mutex<Option<EchoCanceller3>>,
    mixer: Mutex<Mixer>,
    /// Entrées des canaux, fermées à l'arrêt (les rappels du serveur son peuvent survivre un instant).
    me_feed: Mutex<Option<PipelineFeed>>,
    them_feed: Mutex<Option<PipelineFeed>>,
}

#[derive(Default)]
struct MicState {
    stream: Option<u64>,
    /// Micro demandé (`None` = défaut).
    wanted: Option<String>,
    /// Micro demandé introuvable : le défaut le remplace.
    fallback: bool,
    /// Prochain essai après une perte du flux.
    retry_at: Option<Instant>,
    failures: u32,
}

#[derive(Default)]
struct AppState {
    targets: TapTargets,
    streams: HashMap<PlaybackStream, u64>,
    global: Option<(u64, String)>,
    waiting: bool,
}

pub struct Session {
    cfg: SessionConfig,
    pulse: Pulse,
    pub start_instant: Instant,
    me: ChannelPipeline,
    them: ChannelPipeline,
    shared: Arc<Shared>,
    mic: Mutex<MicState>,
    apps: Mutex<AppState>,
    levels_stop: Arc<AtomicBool>,
    levels: Option<std::thread::JoinHandle<()>>,
    errors: Arc<RateLimiter>,
}

fn report(errors: &RateLimiter, code: ErrorCode, message: String, recoverable: bool) {
    if errors.allow(code.as_str()) {
        log::send(&Outgoing::Error { code, message, recoverable });
    }
}

impl Session {
    pub fn start(cfg: SessionConfig, pulse: Pulse, models: SpeechModels, muted: bool, devices: &DeviceSnapshot, targets: TapTargets) -> Session {
        let start_instant = Instant::now();
        let start_wall = chrono::Local::now();
        let meter = Arc::new(LevelMeter::default());
        let errors = Arc::new(RateLimiter::new(30));

        let (mut me_wav, mut them_wav) = (None, None);
        if cfg.keep_audio {
            match &cfg.audio_dir {
                Some(dir) => match (WavWriter::create(&dir.join("me.wav"), 16_000), WavWriter::create(&dir.join("them.wav"), 16_000)) {
                    (Ok(a), Ok(b)) => {
                        me_wav = Some(a);
                        them_wav = Some(b);
                        log::info(&format!("audio conservé dans {}", dir.display()));
                    }
                    (Err(e), _) | (_, Err(e)) => log::error(&format!("impossible de créer les WAV dans {} : {e:#}", dir.display())),
                },
                None => log::warn("keep_audio sans audio_dir : aucun fichier audio ne sera écrit"),
            }
        }

        let flags: Arc<Mutex<Flags>> = Arc::new(Mutex::new(Flags { muted, ..Default::default() }));
        let make = |channel: Channel, wav: Option<WavWriter>| {
            let flags = flags.clone();
            let errors = errors.clone();
            ChannelPipeline::start(PipelineConfig {
                channel,
                session_start: start_instant,
                silence_ms: cfg.silence_ms,
                vocabulary: cfg.vocabulary.clone(),
                models: models.clone(),
                wav,
                meter: meter.clone(),
                on_segment: Arc::new(move |u, text| {
                    let private = channel == Channel::Me && {
                        let f = flags.lock().unwrap();
                        f.aside.iter().any(|(s, e)| *s <= u.t1() && e.unwrap_or(f64::INFINITY) >= u.t0())
                    };
                    let ts = iso8601(start_wall + chrono::Duration::milliseconds((u.t1() * 1000.0) as i64));
                    log::send(&Outgoing::Segment(SegmentPayload { ts, t0: u.t0(), t1: u.t1(), channel, text, private }));
                }),
                on_error: Arc::new(move |code, msg, recoverable| {
                    if code == ErrorCode::ModelMissing {
                        log::send(&Outgoing::Error { code, message: msg, recoverable });
                    } else {
                        report(&errors, code, msg, recoverable);
                    }
                }),
            })
        };
        let me = make(Channel::Me, me_wav);
        let them = make(Channel::Them, them_wav);
        let aec = if !cfg.room && crate::aec::enabled_by_env() { EchoCanceller3::new() } else { None };
        if !cfg.room {
            log::info(if aec.is_some() { "annulation d'écho active (WebRTC AEC3)" } else { "annulation d'écho inactive" });
        }
        // Le même verrou sert aux rappels du serveur son et aux segments (aparté).
        let shared = Arc::new(Shared { flags: flags.clone(), aec: Mutex::new(aec), mixer: Mutex::new(Mixer::default()), me_feed: Mutex::new(Some(me.sender())), them_feed: Mutex::new(Some(them.sender())) });

        // Niveaux toutes les 250 ms.
        let levels_stop = Arc::new(AtomicBool::new(false));
        let levels = {
            let stop = levels_stop.clone();
            let meter = meter.clone();
            std::thread::Builder::new()
                .name("steno.levels".into())
                .spawn(move || {
                    while !stop.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(250));
                        let (me, them) = meter.take();
                        log::send(&Outgoing::Level { me, them });
                    }
                })
                .ok()
        };

        let s = Session {
            cfg: cfg.clone(),
            pulse,
            start_instant,
            me,
            them,
            shared,
            mic: Mutex::new(MicState { wanted: cfg.input_device.clone(), ..Default::default() }),
            apps: Mutex::new(AppState { targets: targets.clone(), ..Default::default() }),
            levels_stop,
            levels,
            errors,
        };
        s.open_mic(devices);

        if cfg.room {
            log::info("réunion en présentiel : micro seul");
        } else {
            s.reconcile(devices);
            if !global_mode() && targets.streams.is_empty() {
                let message = if targets.app_names.is_empty() {
                    "Aucune app de visio n'est surveillée : seul le micro est capté, pas le son des participants.".to_string()
                } else {
                    let l = targets.label();
                    format!("Aucun flux audio de {l} : l'app n'est pas lancée ou n'a pas encore ouvert l'audio. Le micro est capté ; le son des participants le sera dès que {l} jouera du son.")
                };
                s.apps.lock().unwrap().waiting = true;
                report(&s.errors, ErrorCode::TapFailed, message, true);
            }
        }
        s
    }

    // MARK: Micro

    fn mic_sink(&self) -> crate::pulse::DataSink {
        let shared = self.shared.clone();
        Box::new(move |samples: Vec<f32>| {
            let processed = match shared.aec.lock().unwrap().as_mut() {
                Some(aec) => aec.capture(&samples),
                None => samples,
            };
            if processed.is_empty() {
                return;
            }
            let mut f = shared.flags.lock().unwrap();
            for c in f.collectors.values_mut() {
                c.extend_from_slice(&processed);
            }
            // Micro coupé ou pause : du silence à la place (VAD muette, WAV et horloge continus).
            let silent = f.muted || f.paused;
            drop(f);
            if let Some(feed) = shared.me_feed.lock().unwrap().as_ref() {
                feed.feed(if silent { vec![0.0; processed.len()] } else { processed });
            }
        })
    }

    fn open_mic(&self, devices: &DeviceSnapshot) {
        let mut m = self.mic.lock().unwrap();
        if let Some(id) = m.stream.take() {
            self.pulse.close(id);
        }
        let mut device = m.wanted.clone();
        if let Some(uid) = &m.wanted {
            if !devices.inputs.iter().any(|d| &d.id == uid) {
                if !m.fallback {
                    log::send(&Outgoing::Error { code: ErrorCode::DeviceMissing, message: format!("Micro « {uid} » introuvable : micro par défaut utilisé."), recoverable: true });
                }
                m.fallback = true;
                device = None;
            } else {
                m.fallback = false;
            }
        }
        match self.pulse.open(RecordSource::Source(device.clone()), self.mic_sink()) {
            Ok((id, name)) => {
                log::info(&format!("micro ouvert : {name}"));
                m.stream = Some(id);
                m.failures = 0;
                m.retry_at = None;
            }
            Err(e) => {
                m.failures += 1;
                m.retry_at = Some(Instant::now() + Duration::from_secs(5));
                report(&self.errors, ErrorCode::DeviceChanged, format!("Démarrage du micro impossible ({e:#}). Nouvel essai toutes les 5 s."), true);
            }
        }
    }

    /// Change de micro sans interrompre la capture. `None` = micro par défaut.
    pub fn set_input(&self, uid: Option<String>, devices: &DeviceSnapshot) {
        {
            let mut m = self.mic.lock().unwrap();
            m.wanted = uid;
            m.fallback = false;
        }
        self.open_mic(devices);
    }

    // MARK: Apps captées

    fn app_sink(&self, id_cell: Arc<std::sync::atomic::AtomicU64>) -> crate::pulse::DataSink {
        let shared = self.shared.clone();
        Box::new(move |samples: Vec<f32>| {
            let id = id_cell.load(Ordering::SeqCst);
            let mixed = shared.mixer.lock().unwrap().push(id, samples);
            if mixed.is_empty() {
                return;
            }
            if let Some(aec) = shared.aec.lock().unwrap().as_mut() {
                aec.render(&mixed);
            }
            let paused = shared.flags.lock().unwrap().paused;
            if let Some(feed) = shared.them_feed.lock().unwrap().as_ref() {
                feed.feed(if paused { vec![0.0; mixed.len()] } else { mixed });
            }
        })
    }

    fn open_app_stream(&self, source: RecordSource) -> anyhow::Result<u64> {
        // L'identifiant n'est connu qu'après l'ouverture : le rappel le lit dans une cellule.
        let cell = Arc::new(std::sync::atomic::AtomicU64::new(u64::MAX));
        let (id, _) = self.pulse.open(source, self.app_sink(cell.clone()))?;
        cell.store(id, Ordering::SeqCst);
        Ok(id)
    }

    /// Nouvelles cibles (relevé du moniteur toutes les 2 s, ou `watch`).
    pub fn update_targets(&self, targets: TapTargets, devices: &DeviceSnapshot) {
        {
            let mut a = self.apps.lock().unwrap();
            if a.targets == targets {
                return;
            }
            if a.targets.app_names != targets.app_names {
                log::info(&format!("apps captées : {}", targets.label()));
            }
            a.targets = targets;
        }
        self.reconcile(devices);
    }

    /// Ouvre les flux voulus, ferme les autres.
    pub fn reconcile(&self, devices: &DeviceSnapshot) {
        if self.cfg.room {
            return;
        }
        let mut a = self.apps.lock().unwrap();
        if global_mode() {
            let Some(sink) = devices.default_sink.as_ref().and_then(|n| devices.sinks.iter().find(|s| &s.name == n)) else { return };
            if a.global.as_ref().map(|g| g.1 == sink.monitor_source).unwrap_or(false) {
                return;
            }
            if let Some((id, _)) = a.global.take() {
                self.pulse.close(id);
                self.shared.mixer.lock().unwrap().remove(id);
            }
            match self.open_app_stream(RecordSource::Monitor(sink.monitor_source.clone())) {
                Ok(id) => {
                    log::info(&format!("capture de tout le son de « {} » (mode global)", sink.name));
                    a.global = Some((id, sink.monitor_source.clone()));
                }
                Err(e) => report(&self.errors, ErrorCode::TapFailed, format!("Capture du son du système impossible ({e:#}). Nouvel essai en cours."), true),
            }
            return;
        }
        let desired = a.targets.streams.clone();
        let stale: Vec<PlaybackStream> = a.streams.keys().filter(|k| !desired.contains(k)).copied().collect();
        for k in stale {
            if let Some(id) = a.streams.remove(&k) {
                self.pulse.close(id);
                self.shared.mixer.lock().unwrap().remove(id);
            }
        }
        for (index, sink) in desired {
            if a.streams.contains_key(&(index, sink)) {
                continue;
            }
            let Some(monitor) = devices.sinks.iter().find(|s| s.index == sink).map(|s| s.monitor_source.clone()) else { continue };
            match self.open_app_stream(RecordSource::SinkInput { index, monitor_source: monitor }) {
                Ok(id) => {
                    a.streams.insert((index, sink), id);
                }
                Err(e) => {
                    let label = a.targets.label();
                    report(&self.errors, ErrorCode::TapFailed, format!("Capture du son de {label} impossible ({e:#}). Nouvel essai en cours."), true);
                }
            }
        }
        if a.waiting && !a.streams.is_empty() {
            a.waiting = false;
            log::info(&format!("flux audio de {} apparus : capture du son des participants démarrée", a.targets.label()));
        }
    }

    // MARK: Événements du serveur son

    /// Flux terminé par le serveur (micro débranché, flux d'app fermé…).
    pub fn stream_ended(&self, id: u64, devices: &DeviceSnapshot) {
        let mic_lost = {
            let mut m = self.mic.lock().unwrap();
            if m.stream == Some(id) {
                m.stream = None;
                true
            } else {
                false
            }
        };
        if mic_lost {
            log::info("flux du micro terminé : reprise");
            self.open_mic(devices);
            return;
        }
        let mut a = self.apps.lock().unwrap();
        if let Some(k) = a.streams.iter().find(|(_, v)| **v == id).map(|(k, _)| *k) {
            a.streams.remove(&k);
            self.shared.mixer.lock().unwrap().remove(id);
        }
        if a.global.as_ref().map(|g| g.0) == Some(id) {
            a.global = None;
            self.shared.mixer.lock().unwrap().remove(id);
        }
    }

    /// Liste des périphériques modifiée : micro demandé revenu, sortie par défaut changée…
    pub fn devices_changed(&self, devices: &DeviceSnapshot) {
        let reopen = {
            let m = self.mic.lock().unwrap();
            m.fallback && m.wanted.as_ref().map(|u| devices.inputs.iter().any(|d| &d.id == u)).unwrap_or(false)
        };
        if reopen {
            log::info("micro demandé revenu : reprise");
            self.open_mic(devices);
        }
        self.reconcile(devices);
    }

    /// Connexion au serveur son rétablie : tout est rouvert.
    pub fn reconnected(&self, devices: &DeviceSnapshot) {
        self.mic.lock().unwrap().stream = None;
        {
            let mut a = self.apps.lock().unwrap();
            a.streams.clear();
            a.global = None;
        }
        *self.shared.mixer.lock().unwrap() = Mixer::default();
        self.open_mic(devices);
        self.reconcile(devices);
    }

    /// Appelé chaque seconde : nouvel essai d'ouverture du micro si besoin.
    pub fn tick(&self, devices: impl FnOnce() -> Option<DeviceSnapshot>) {
        let due = {
            let m = self.mic.lock().unwrap();
            m.stream.is_none() && m.retry_at.map(|t| Instant::now() >= t).unwrap_or(false)
        };
        if due {
            if let Some(d) = devices() {
                self.open_mic(&d);
            }
        }
    }

    // MARK: Aparté, pause, micro coupé

    fn now(&self) -> f64 {
        self.start_instant.elapsed().as_secs_f64()
    }

    pub fn set_aside(&self, on: bool) {
        let now = self.now();
        let mut f = self.shared.flags.lock().unwrap();
        if on {
            if f.aside.last().map(|l| l.1.is_some()).unwrap_or(true) {
                f.aside.push((now, None));
            }
        } else if let Some(last) = f.aside.last_mut() {
            if last.1.is_none() {
                last.1 = Some(now);
            }
        }
    }


    pub fn set_paused(&self, on: bool) {
        self.shared.flags.lock().unwrap().paused = on;
    }

    pub fn set_muted(&self, on: bool) {
        self.shared.flags.lock().unwrap().muted = on;
    }

    /// Enregistre `seconds` secondes du micro déjà ouvert (test du mot d'appel en réunion).
    pub fn collector_start(&self) -> u64 {
        let mut f = self.shared.flags.lock().unwrap();
        f.next_collector += 1;
        let id = f.next_collector;
        f.collectors.insert(id, vec![]);
        id
    }

    pub fn collector_take(&self, id: u64) -> Vec<f32> {
        self.shared.flags.lock().unwrap().collectors.remove(&id).unwrap_or_default()
    }

    // MARK: Arrêt

    /// Arrête la capture puis vide les segments en cours (attend leur transcription).
    pub fn stop(mut self) {
        if let Some(id) = self.mic.lock().unwrap().stream.take() {
            self.pulse.close(id);
        }
        {
            let mut a = self.apps.lock().unwrap();
            for (_, id) in a.streams.drain() {
                self.pulse.close(id);
            }
            if let Some((id, _)) = a.global.take() {
                self.pulse.close(id);
            }
        }
        // Plus aucun échantillon : les entrées des canaux sont fermées, ce qui termine leurs boucles.
        *self.shared.me_feed.lock().unwrap() = None;
        *self.shared.them_feed.lock().unwrap() = None;
        self.levels_stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.levels.take() {
            let _ = h.join();
        }
        // Les deux canaux se vident en parallèle.
        let (me, them) = (&mut self.me, &mut self.them);
        std::thread::scope(|sc| {
            sc.spawn(|| them.finish());
            me.finish();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn melangeur() {
        let mut m = Mixer::default();
        assert_eq!(m.push(1, vec![0.1; 10]), vec![0.1; 10]);
        assert!(m.push(2, vec![0.2; 5]).is_empty());
        let out = m.push(1, vec![0.1; 10]);
        assert_eq!(out.len(), 10);
        assert!((out[0] - 0.3).abs() < 1e-6 && (out[9] - 0.1).abs() < 1e-6);
        // Le flux principal se tait : l'autre prend le relais au-delà de 0,2 s d'avance.
        assert!(m.push(2, vec![0.2; 4_000]).len() == 4_000);
        m.remove(2);
        assert_eq!(m.push(1, vec![0.5; 3]), vec![0.5; 3]);
    }
}
