//! Accès au serveur son par l'API PulseAudio (servie aussi par PipeWire via pipewire-pulse,
//! le cas par défaut sur Ubuntu 22.04+ et Debian 12+).
//!
//! Un thread dédié possède la boucle PulseAudio et tous ses objets (non `Send`) ; le reste du
//! sidecar lui parle par un canal de requêtes ([`Pulse`]) et reçoit ses notifications par un
//! canal d'événements ([`PulseEvent`]). Il fournit :
//! - la liste des micros (sources hors moniteurs) et des sorties (puits), défauts compris ;
//! - la liste des flux des applications (lecture : sink inputs ; enregistrement : source outputs)
//!   avec leurs propriétés (PID, exécutable, nom, identifiant d'app) ;
//! - des flux d'enregistrement 16 kHz mono Float32 (le serveur rééchantillonne et mélange) : micro,
//!   moniteur d'**un seul flux d'application** (`pa_stream_set_monitor_stream`, l'équivalent du
//!   process tap de macOS) ou moniteur d'une sortie entière (plan B « global ») ;
//! - la lecture d'un court signal sur une sortie donnée.
//! Si le serveur disparaît (redémarrage de PipeWire), le thread se reconnecte toutes les 2 s.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use libpulse_binding as pa;
use pa::callbacks::ListResult;
use pa::context::subscribe::{Facility, InterestMaskSet};
use pa::context::{Context, FlagSet as CtxFlags, State as CtxState};
use pa::def::BufferAttr;
use pa::mainloop::standard::Mainloop;
use pa::proplist::Proplist;
use pa::sample::{Format, Spec};
use pa::stream::{FlagSet as StreamFlags, PeekResult, SeekMode, State as StreamState, Stream};
use pa::time::MicroSeconds;

use crate::devices::{transport, NodeProps};
use crate::protocol::AudioDeviceInfo;

pub const RATE: u32 = 16_000;

/// Sortie (puits) : son moniteur sert à capter un flux d'application qui y joue.
#[derive(Debug, Clone)]
pub struct SinkLite {
    pub index: u32,
    pub name: String,
    pub monitor_source: String,
    /// Casque ou écouteurs (port actif, type d'appareil, Bluetooth).
    pub headphones: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceSnapshot {
    pub inputs: Vec<AudioDeviceInfo>,
    pub outputs: Vec<AudioDeviceInfo>,
    pub sinks: Vec<SinkLite>,
    pub default_source: Option<String>,
    pub default_sink: Option<String>,
}

impl DeviceSnapshot {
    /// La sortie par défaut est-elle un casque ? `None` si elle est inconnue.
    pub fn default_output_is_headphones(&self) -> Option<bool> {
        let name = self.default_sink.as_ref()?;
        self.sinks.iter().find(|s| &s.name == name).map(|s| s.headphones)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    /// Flux de lecture (sink input) : l'app joue du son.
    Playback,
    /// Flux d'enregistrement (source output) sur un vrai micro : l'app écoute.
    Record,
}

/// Flux audio d'une application.
#[derive(Debug, Clone)]
pub struct AudioClient {
    pub kind: ClientKind,
    pub index: u32,
    pub pid: Option<u32>,
    pub binary: Option<String>,
    pub app_name: Option<String>,
    /// `application.id`, identifiant de portail Flatpak, etc.
    pub app_ids: Vec<String>,
    /// Lecture : index du puits où joue le flux.
    pub sink: Option<u32>,
    pub corked: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PulseEvent {
    /// Connexion établie (ou rétablie).
    Connected,
    /// Serveur perdu : tous les flux sont fermés, reconnexion en cours.
    Disconnected,
    DevicesChanged,
    ClientsChanged,
    /// Flux d'enregistrement terminé par le serveur (périphérique débranché, flux suivi fermé…).
    StreamEnded(u64),
}

/// Ce qu'enregistre un flux.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordSource {
    /// Micro (`None` = source par défaut, suivie par le serveur).
    Source(Option<String>),
    /// Un flux d'application précis, via le moniteur de la sortie où il joue.
    SinkInput { index: u32, monitor_source: String },
    /// Tout le son d'une sortie (moniteur).
    Monitor(String),
}

/// Reçoit les échantillons d'un flux d'enregistrement (appelé sur le thread PulseAudio : ne doit pas bloquer).
pub type DataSink = Box<dyn FnMut(Vec<f32>) + Send>;

enum Req {
    Devices(Sender<Result<DeviceSnapshot>>),
    Clients(Sender<Result<Vec<AudioClient>>>),
    Open { id: u64, source: RecordSource, data: DataSink, reply: Sender<Result<String>> },
    Close(u64),
    Play { device: Option<String>, samples: Vec<f32>, rate: u32 },
}

/// Poignée vers le thread PulseAudio (clonable, `Send`).
#[derive(Clone)]
pub struct Pulse {
    tx: Sender<Req>,
    next_id: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

const TIMEOUT: Duration = Duration::from_secs(5);

impl Pulse {
    /// Lance le thread ; les événements arrivent sur le récepteur renvoyé.
    pub fn start() -> (Pulse, Receiver<PulseEvent>) {
        let (tx, rx) = unbounded();
        let (ev_tx, ev_rx) = unbounded();
        std::thread::Builder::new().name("steno.pulse".into()).spawn(move || run(rx, ev_tx)).expect("thread PulseAudio");
        (Pulse { tx, next_id: Default::default() }, ev_rx)
    }

    pub fn devices(&self) -> Result<DeviceSnapshot> {
        let (tx, rx) = bounded(1);
        self.tx.send(Req::Devices(tx)).map_err(|_| anyhow!("thread PulseAudio arrêté"))?;
        rx.recv_timeout(TIMEOUT).map_err(|_| anyhow!("serveur son injoignable"))?
    }

    pub fn clients(&self) -> Result<Vec<AudioClient>> {
        let (tx, rx) = bounded(1);
        self.tx.send(Req::Clients(tx)).map_err(|_| anyhow!("thread PulseAudio arrêté"))?;
        rx.recv_timeout(TIMEOUT).map_err(|_| anyhow!("serveur son injoignable"))?
    }

    /// Ouvre un flux d'enregistrement ; renvoie son identifiant et le nom du périphérique réel.
    pub fn open(&self, source: RecordSource, data: DataSink) -> Result<(u64, String)> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let (tx, rx) = bounded(1);
        self.tx.send(Req::Open { id, source, data, reply: tx }).map_err(|_| anyhow!("thread PulseAudio arrêté"))?;
        let device = rx.recv_timeout(TIMEOUT).map_err(|_| anyhow!("serveur son injoignable"))??;
        Ok((id, device))
    }

    pub fn close(&self, id: u64) {
        let _ = self.tx.send(Req::Close(id));
    }

    /// Joue des échantillons mono sur une sortie (`None` = défaut). Ne bloque pas.
    pub fn play(&self, device: Option<String>, samples: Vec<f32>, rate: u32) {
        let _ = self.tx.send(Req::Play { device, samples, rate });
    }
}

// MARK: - Thread

struct Open {
    stream: Rc<RefCell<Stream>>,
}

fn prop(p: &Proplist, k: &str) -> Option<String> {
    p.get_str(k).filter(|s| !s.trim().is_empty())
}

fn node_props(name: &str, p: &Proplist, driver: Option<&str>) -> NodeProps {
    NodeProps {
        name: name.to_string(),
        bus: prop(p, "device.bus"),
        api: prop(p, "device.api"),
        form_factor: prop(p, "device.form_factor"),
        class: prop(p, "device.class"),
        driver: driver.map(String::from).or_else(|| prop(p, "node.driver")),
        profile: prop(p, "device.profile.name"),
    }
}

fn client_props(p: &Proplist) -> (Option<u32>, Option<String>, Option<String>, Vec<String>) {
    let pid = prop(p, "application.process.id").and_then(|s| s.trim().parse().ok());
    let binary = prop(p, "application.process.binary");
    let name = prop(p, "application.name");
    let ids = ["application.id", "pipewire.access.portal.app_id", "application.flatpak.id", "application.icon_name"].iter().filter_map(|k| prop(p, k)).collect();
    (pid, binary, name, ids)
}

fn run(rx: Receiver<Req>, events: Sender<PulseEvent>) {
    let mut announced_down = false;
    loop {
        let mut connected = false;
        let result = session(&rx, &events, &mut connected);
        if connected {
            announced_down = false;
        }
        match result {
            Ok(()) => return, // canal de requêtes fermé : arrêt
            Err(e) => {
                if !announced_down {
                    crate::log::warn(&format!("serveur son : {e:#} ; nouvel essai toutes les 2 s"));
                    announced_down = true;
                }
                let _ = events.send(PulseEvent::Disconnected);
            }
        }
        // En attendant : répondre aux requêtes par une erreur, sans bloquer les appelants.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(req) => fail(req),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => break,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

fn fail(req: Req) {
    let e = || anyhow!("serveur son indisponible (PulseAudio / PipeWire)");
    match req {
        Req::Devices(r) => drop(r.send(Err(e()))),
        Req::Clients(r) => drop(r.send(Err(e()))),
        Req::Open { reply, .. } => drop(reply.send(Err(e()))),
        Req::Close(_) | Req::Play { .. } => {}
    }
}

/// Une connexion : renvoie `Ok` quand le canal de requêtes est fermé, `Err` si le serveur est perdu.
fn session(rx: &Receiver<Req>, events: &Sender<PulseEvent>, connected: &mut bool) -> Result<()> {
    let mut ml = Mainloop::new().ok_or_else(|| anyhow!("boucle PulseAudio"))?;
    let mut pl = Proplist::new().ok_or_else(|| anyhow!("proplist"))?;
    let _ = pl.set_str("application.name", "Sténo");
    let _ = pl.set_str("application.id", "fr.nakile.steno");
    let _ = pl.set_str("application.icon_name", "steno");
    let ctx = Rc::new(RefCell::new(Context::new_with_proplist(&ml, "Sténo", &pl).ok_or_else(|| anyhow!("contexte PulseAudio"))?));
    ctx.borrow_mut().connect(None, CtxFlags::NOFLAGS, None).map_err(|e| anyhow!("connexion : {e}"))?;
    let started = std::time::Instant::now();
    loop {
        iterate(&mut ml)?;
        match ctx.borrow().get_state() {
            CtxState::Ready => break,
            CtxState::Failed | CtxState::Terminated => return Err(anyhow!("connexion refusée")),
            _ => {}
        }
        if started.elapsed() > Duration::from_secs(10) {
            return Err(anyhow!("délai de connexion dépassé"));
        }
    }
    *connected = true;
    crate::log::info("serveur son connecté");
    let _ = events.send(PulseEvent::Connected);

    {
        let ev = events.clone();
        ctx.borrow_mut().set_subscribe_callback(Some(Box::new(move |facility, _op, _idx| {
            let e = match facility {
                Some(Facility::Sink) | Some(Facility::Source) | Some(Facility::Server) | Some(Facility::Card) => PulseEvent::DevicesChanged,
                Some(Facility::SinkInput) | Some(Facility::SourceOutput) => PulseEvent::ClientsChanged,
                _ => return,
            };
            let _ = ev.send(e);
        })));
        let mask = InterestMaskSet::SINK | InterestMaskSet::SOURCE | InterestMaskSet::SINK_INPUT | InterestMaskSet::SOURCE_OUTPUT | InterestMaskSet::SERVER | InterestMaskSet::CARD;
        ctx.borrow_mut().subscribe(mask, |_| {});
    }

    let mut streams: HashMap<u64, Open> = HashMap::new();
    let ended: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(vec![]));
    let mut players: Vec<Rc<RefCell<Stream>>> = vec![];

    loop {
        iterate(&mut ml)?;
        if !matches!(ctx.borrow().get_state(), CtxState::Ready) {
            for id in streams.keys() {
                let _ = events.send(PulseEvent::StreamEnded(*id));
            }
            return Err(anyhow!("connexion perdue"));
        }
        for id in ended.borrow_mut().drain(..) {
            if let Some(o) = streams.remove(&id) {
                let _ = o.stream.borrow_mut().disconnect();
                let _ = events.send(PulseEvent::StreamEnded(id));
            }
        }
        players.retain(|p| !matches!(p.borrow().get_state(), StreamState::Terminated | StreamState::Failed));
        loop {
            let req = match rx.try_recv() {
                Ok(r) => r,
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    for (_, o) in streams.drain() {
                        let _ = o.stream.borrow_mut().disconnect();
                    }
                    ctx.borrow_mut().disconnect();
                    return Ok(());
                }
            };
            match req {
                Req::Devices(reply) => list_devices(&ctx, reply),
                Req::Clients(reply) => list_clients(&ctx, reply),
                Req::Open { id, source, data, reply } => match open_record(&ctx, id, &source, data, ended.clone()) {
                    Ok((s, device)) => {
                        streams.insert(id, Open { stream: s });
                        let _ = reply.send(Ok(device));
                    }
                    Err(e) => drop(reply.send(Err(e))),
                },
                Req::Close(id) => {
                    if let Some(o) = streams.remove(&id) {
                        let mut s = o.stream.borrow_mut();
                        s.set_read_callback(None);
                        s.set_state_callback(None);
                        let _ = s.disconnect();
                    }
                }
                Req::Play { device, samples, rate } => match play(&ctx, device.as_deref(), samples, rate) {
                    Ok(s) => players.push(s),
                    Err(e) => crate::log::warn(&format!("signal sonore : {e:#}")),
                },
            }
        }
    }
}

fn iterate(ml: &mut Mainloop) -> Result<()> {
    ml.prepare(Some(MicroSeconds(20_000))).map_err(|e| anyhow!("boucle PulseAudio : {e}"))?;
    ml.poll().map_err(|e| anyhow!("boucle PulseAudio : {e}"))?;
    ml.dispatch().map_err(|e| anyhow!("boucle PulseAudio : {e}"))?;
    Ok(())
}

#[derive(Default)]
struct DevAcc {
    snap: DeviceSnapshot,
    pending: u8,
    error: Option<String>,
}

fn list_devices(ctx: &Rc<RefCell<Context>>, reply: Sender<Result<DeviceSnapshot>>) {
    let acc = Rc::new(RefCell::new(DevAcc { pending: 3, ..Default::default() }));
    let reply = Rc::new(reply);
    let finish = {
        let acc = acc.clone();
        let reply = reply.clone();
        move || {
            let mut a = acc.borrow_mut();
            a.pending -= 1;
            if a.pending == 0 {
                if let Some(e) = a.error.take() {
                    let _ = reply.send(Err(anyhow!(e)));
                    return;
                }
                let mut snap = std::mem::take(&mut a.snap);
                let ds = snap.default_source.clone();
                let dk = snap.default_sink.clone();
                for d in snap.inputs.iter_mut() {
                    d.is_default = Some(&d.id) == ds.as_ref();
                }
                for d in snap.outputs.iter_mut() {
                    d.is_default = Some(&d.id) == dk.as_ref();
                }
                snap.inputs = crate::devices::cleaned(snap.inputs);
                snap.outputs = crate::devices::cleaned(snap.outputs);
                let _ = reply.send(Ok(snap));
            }
        }
    };
    let finish = Rc::new(finish);
    let intro = ctx.borrow().introspect();
    {
        let (acc, finish) = (acc.clone(), finish.clone());
        intro.get_server_info(move |info| {
            {
                let mut a = acc.borrow_mut();
                a.snap.default_source = info.default_source_name.as_ref().map(|s| s.to_string());
                a.snap.default_sink = info.default_sink_name.as_ref().map(|s| s.to_string());
            }
            finish();
        });
    }
    {
        let (acc, finish) = (acc.clone(), finish.clone());
        intro.get_source_info_list(move |r| match r {
            ListResult::Item(s) => {
                // Les moniteurs (son d'une sortie) ne sont pas des micros.
                if s.monitor_of_sink.is_some() {
                    return;
                }
                let name = s.name.as_deref().unwrap_or("").to_string();
                let props = node_props(&name, &s.proplist, s.driver.as_deref());
                acc.borrow_mut().snap.inputs.push(AudioDeviceInfo {
                    id: name.clone(),
                    name: s.description.as_deref().unwrap_or(&name).to_string(),
                    is_default: false,
                    transport: transport(&props).into(),
                });
            }
            ListResult::End => finish(),
            ListResult::Error => {
                acc.borrow_mut().error = Some("liste des micros".into());
                finish();
            }
        });
    }
    {
        let (acc, finish) = (acc.clone(), finish.clone());
        intro.get_sink_info_list(move |r| match r {
            ListResult::Item(s) => {
                let name = s.name.as_deref().unwrap_or("").to_string();
                let props = node_props(&name, &s.proplist, s.driver.as_deref());
                let mut a = acc.borrow_mut();
                let port = s.active_port.as_ref().and_then(|p| p.name.as_deref().map(String::from));
                let headphones = crate::devices::is_headphones(&props, port.as_deref());
                a.snap.sinks.push(SinkLite { index: s.index, name: name.clone(), monitor_source: s.monitor_source_name.as_deref().unwrap_or("").to_string(), headphones });
                a.snap.outputs.push(AudioDeviceInfo {
                    id: name.clone(),
                    name: s.description.as_deref().unwrap_or(&name).to_string(),
                    is_default: false,
                    transport: transport(&props).into(),
                });
            }
            ListResult::End => finish(),
            ListResult::Error => {
                acc.borrow_mut().error = Some("liste des sorties".into());
                finish();
            }
        });
    }
}

#[derive(Default)]
struct CliAcc {
    clients: Vec<AudioClient>,
    /// Source outputs : (client, index de la source).
    records: Vec<(AudioClient, u32)>,
    monitor_sources: Vec<u32>,
    pending: u8,
    error: Option<String>,
}

fn list_clients(ctx: &Rc<RefCell<Context>>, reply: Sender<Result<Vec<AudioClient>>>) {
    let acc = Rc::new(RefCell::new(CliAcc { pending: 3, ..Default::default() }));
    let finish = {
        let acc = acc.clone();
        Rc::new(move || {
            let mut a = acc.borrow_mut();
            a.pending -= 1;
            if a.pending == 0 {
                if let Some(e) = a.error.take() {
                    let _ = reply.send(Err(anyhow!(e)));
                    return;
                }
                let mut out = std::mem::take(&mut a.clients);
                let monitors = std::mem::take(&mut a.monitor_sources);
                // Un enregistrement du moniteur d'une sortie (capture d'écran, autre outil de
                // transcription…) n'est pas un usage du micro.
                out.extend(std::mem::take(&mut a.records).into_iter().filter(|(_, src)| !monitors.contains(src)).map(|(c, _)| c));
                let _ = reply.send(Ok(out));
            }
        })
    };
    let intro = ctx.borrow().introspect();
    {
        let (acc, finish) = (acc.clone(), finish.clone());
        intro.get_sink_input_info_list(move |r| match r {
            ListResult::Item(i) => {
                let (pid, binary, app_name, app_ids) = client_props(&i.proplist);
                acc.borrow_mut().clients.push(AudioClient { kind: ClientKind::Playback, index: i.index, pid, binary, app_name, app_ids, sink: Some(i.sink), corked: i.corked });
            }
            ListResult::End => finish(),
            ListResult::Error => {
                acc.borrow_mut().error = Some("liste des flux de lecture".into());
                finish();
            }
        });
    }
    {
        let (acc, finish) = (acc.clone(), finish.clone());
        intro.get_source_output_info_list(move |r| match r {
            ListResult::Item(o) => {
                let (pid, binary, app_name, app_ids) = client_props(&o.proplist);
                acc.borrow_mut().records.push((AudioClient { kind: ClientKind::Record, index: o.index, pid, binary, app_name, app_ids, sink: None, corked: o.corked }, o.source));
            }
            ListResult::End => finish(),
            ListResult::Error => {
                acc.borrow_mut().error = Some("liste des flux d'enregistrement".into());
                finish();
            }
        });
    }
    {
        let (acc, finish) = (acc.clone(), finish.clone());
        intro.get_source_info_list(move |r| match r {
            ListResult::Item(s) => {
                if s.monitor_of_sink.is_some() {
                    acc.borrow_mut().monitor_sources.push(s.index);
                }
            }
            ListResult::End => finish(),
            ListResult::Error => {
                acc.borrow_mut().error = Some("liste des sources".into());
                finish();
            }
        });
    }
}

fn spec(rate: u32) -> Spec {
    Spec { format: Format::F32le, rate, channels: 1 }
}

fn open_record(ctx: &Rc<RefCell<Context>>, id: u64, source: &RecordSource, mut data: DataSink, ended: Rc<RefCell<Vec<u64>>>) -> Result<(Rc<RefCell<Stream>>, String)> {
    let name = match source {
        RecordSource::Source(_) => "Sténo — micro",
        RecordSource::SinkInput { .. } => "Sténo — son de l'appel",
        RecordSource::Monitor(_) => "Sténo — son du système",
    };
    let mut pl = Proplist::new().ok_or_else(|| anyhow!("proplist"))?;
    let _ = pl.set_str("media.role", "phone");
    // Les flux de capture du son des apps ne doivent jamais apparaître comme un usage du micro.
    if !matches!(source, RecordSource::Source(_)) {
        let _ = pl.set_str("stream.monitor", "true");
    }
    let stream = Stream::new_with_proplist(&mut ctx.borrow_mut(), name, &spec(RATE), None, &mut pl).ok_or_else(|| anyhow!("création du flux"))?;
    let stream = Rc::new(RefCell::new(stream));
    let device: Option<String> = match source {
        RecordSource::Source(d) => d.clone(),
        RecordSource::SinkInput { index, monitor_source } => {
            stream.borrow_mut().set_monitor_stream(*index).map_err(|e| anyhow!("flux suivi : {e}"))?;
            Some(monitor_source.clone())
        }
        RecordSource::Monitor(m) => Some(m.clone()),
    };
    // Fragments de 20 ms : latence faible, réveils raisonnables.
    let attr = BufferAttr { maxlength: u32::MAX, tlength: u32::MAX, prebuf: u32::MAX, minreq: u32::MAX, fragsize: RATE / 50 * 4 };
    let mut flags = StreamFlags::ADJUST_LATENCY;
    if !matches!(source, RecordSource::Source(None)) {
        flags |= StreamFlags::DONT_MOVE;
    }
    stream.borrow_mut().connect_record(device.as_deref(), Some(&attr), flags).map_err(|e| anyhow!("connexion du flux : {e}"))?;
    {
        let weak = Rc::downgrade(&stream);
        stream.borrow_mut().set_read_callback(Some(Box::new(move |_n| {
            let Some(s) = weak.upgrade() else { return };
            // Rappel appelé depuis la boucle : le flux n'est pas emprunté ailleurs à ce moment.
            let Ok(mut s) = s.try_borrow_mut() else { return };
            loop {
                match s.peek() {
                    Ok(PeekResult::Data(bytes)) => {
                        let samples: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                        data(samples);
                        let _ = s.discard();
                    }
                    Ok(PeekResult::Hole(_)) => {
                        let _ = s.discard();
                    }
                    Ok(PeekResult::Empty) | Err(_) => break,
                }
            }
        })));
    }
    {
        let weak = Rc::downgrade(&stream);
        stream.borrow_mut().set_state_callback(Some(Box::new(move || {
            let Some(s) = weak.upgrade() else { return };
            let Ok(s) = s.try_borrow() else { return };
            if matches!(s.get_state(), StreamState::Failed | StreamState::Terminated) {
                ended.borrow_mut().push(id);
            }
        })));
    }
    let label = device.clone().unwrap_or_else(|| "défaut".into());
    Ok((stream, label))
}

fn play(ctx: &Rc<RefCell<Context>>, device: Option<&str>, samples: Vec<f32>, rate: u32) -> Result<Rc<RefCell<Stream>>> {
    let mut pl = Proplist::new().ok_or_else(|| anyhow!("proplist"))?;
    let _ = pl.set_str("media.role", "event");
    let stream = Stream::new_with_proplist(&mut ctx.borrow_mut(), "Sténo — signal", &spec(rate), None, &mut pl).ok_or_else(|| anyhow!("création du flux"))?;
    let stream = Rc::new(RefCell::new(stream));
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let pending = Rc::new(RefCell::new(Some(bytes)));
    {
        let weak = Rc::downgrade(&stream);
        let pending = pending.clone();
        stream.borrow_mut().set_write_callback(Some(Box::new(move |_n| {
            let Some(s) = weak.upgrade() else { return };
            let Ok(mut s) = s.try_borrow_mut() else { return };
            if let Some(data) = pending.borrow_mut().take() {
                let _ = s.write_copy(&data, 0, SeekMode::Relative);
                let w2 = weak.clone();
                let _ = s.drain(Some(Box::new(move |_ok| {
                    if let Some(s) = w2.upgrade() {
                        if let Ok(mut s) = s.try_borrow_mut() {
                            let _ = s.disconnect();
                        }
                    }
                })));
            }
        })));
    }
    stream.borrow_mut().connect_playback(device, None, StreamFlags::NOFLAGS, None, None).map_err(|e| anyhow!("lecture : {e}"))?;
    Ok(stream)
}
