//! Surveillance des apps de visio (liste fixée par `watch`, Teams seul avant le premier `watch`).
//!
//! Toutes les ~2 s (et aussitôt qu'un flux audio apparaît ou disparaît) :
//! - liste les flux audio des applications (PulseAudio / PipeWire) avec leur PID ;
//! - rattache chaque flux à une app : exécutable, identifiant Flatpak/Snap ou nom d'application
//!   commençant par l'un des préfixes de l'app, sinon descendance (PID parent) d'un processus de l'app ;
//! - une app est « en appel » si l'un de ses processus enregistre un vrai micro (pas un moniteur),
//!   avec anti-rebond à la fin (`CallActivity`) ;
//! - le titre de l'appel est lu sur les fenêtres X11 de l'app (`titles.rs`).
//! État séparé par app : un événement `call` à chaque changement. Fournit aussi les flux de
//! lecture à capter (`CallSnapshot`), l'équivalent des processus du tap Core Audio.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

use crate::procinfo::{self, ProcInfo};
use crate::protocol::WatchedApp;
use crate::pulse::{AudioClient, ClientKind, Pulse};

// MARK: - Relevés

/// Flux de lecture d'une app : (index du sink input, index du puits).
pub type PlaybackStream = (u32, u32);

#[derive(Debug, Clone, PartialEq)]
pub struct AppSnapshot {
    pub app: WatchedApp,
    /// Flux de lecture de l'app (triés), à capter.
    pub streams: Vec<PlaybackStream>,
    /// Processus de l'app (fenêtres).
    pub pids: Vec<u32>,
    /// Un processus de l'app enregistre un micro.
    pub input_active: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CallSnapshot {
    pub apps: Vec<AppSnapshot>,
}

impl CallSnapshot {
    /// Union triée, sans doublon, des flux de lecture des apps `ids`.
    pub fn streams(&self, ids: &[String]) -> Vec<PlaybackStream> {
        let mut v: Vec<PlaybackStream> = self.apps.iter().filter(|a| ids.contains(&a.app.id)).flat_map(|a| a.streams.clone()).collect();
        v.sort();
        v.dedup();
        v
    }

    pub fn watched(&self) -> Vec<WatchedApp> {
        self.apps.iter().map(|a| a.app.clone()).collect()
    }
}

// MARK: - Anti-rebond (pur, testé)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    None,
    Started,
    Ended,
}

/// Début dès qu'un processus enregistre le micro, fin après `inactive_polls_to_end` relevés
/// négatifs consécutifs.
#[derive(Debug, Clone)]
pub struct CallActivity {
    pub active: bool,
    inactive_polls: u32,
    inactive_polls_to_end: u32,
}

impl Default for CallActivity {
    fn default() -> Self {
        CallActivity { active: false, inactive_polls: 0, inactive_polls_to_end: 2 }
    }
}

impl CallActivity {
    pub fn update(&mut self, input_active: bool) -> Transition {
        if input_active {
            self.inactive_polls = 0;
            if self.active {
                return Transition::None;
            }
            self.active = true;
            return Transition::Started;
        }
        if !self.active {
            return Transition::None;
        }
        self.inactive_polls += 1;
        if self.inactive_polls < self.inactive_polls_to_end {
            return Transition::None;
        }
        self.active = false;
        self.inactive_polls = 0;
        Transition::Ended
    }
}

/// `start.apps` résolu contre la liste surveillée : les apps demandées connues, ou toutes les
/// apps surveillées si la demande est vide ou ne désigne aucune app connue.
pub fn resolve_capture(requested: &[String], watched: &[WatchedApp]) -> (Vec<WatchedApp>, Vec<String>) {
    let known: Vec<WatchedApp> = requested.iter().filter_map(|id| watched.iter().find(|a| &a.id == id).cloned()).collect();
    let unknown: Vec<String> = requested.iter().filter(|id| !watched.iter().any(|a| &a.id == *id)).cloned().collect();
    (if known.is_empty() { watched.to_vec() } else { known }, unknown)
}

// MARK: - Attribution (pure, testée)

/// App de plus long préfixe parmi des noms candidats.
fn match_names<'a>(names: &[String], apps: &'a [WatchedApp]) -> Option<&'a WatchedApp> {
    let mut best: Option<(&WatchedApp, usize)> = None;
    for n in names {
        if let Some((a, len)) = WatchedApp::best_match(n, apps) {
            if len > best.map(|b| b.1).unwrap_or(0) {
                best = Some((a, len));
            }
        }
    }
    best.map(|b| b.0)
}

/// App du processus `pid` ou de son premier ancêtre reconnu (16 niveaux au plus).
fn owner_by_ancestry<'a>(pid: u32, procs: &HashMap<u32, ProcInfo>, apps: &'a [WatchedApp]) -> Option<&'a WatchedApp> {
    let mut current = pid;
    for _ in 0..16 {
        let p = procs.get(&current)?;
        if let Some(a) = match_names(&p.names, apps) {
            return Some(a);
        }
        match p.ppid {
            Some(pp) if pp > 1 && pp != current => current = pp,
            _ => return None,
        }
    }
    None
}

/// Rattache un flux audio à une app.
pub fn attribute<'a>(client: &AudioClient, procs: &HashMap<u32, ProcInfo>, apps: &'a [WatchedApp]) -> Option<&'a WatchedApp> {
    let mut names: Vec<String> = client.binary.iter().cloned().collect();
    names.extend(client.app_ids.iter().cloned());
    if let Some(a) = match_names(&names, apps) {
        return Some(a);
    }
    if let Some(a) = client.pid.and_then(|pid| owner_by_ancestry(pid, procs, apps)) {
        return Some(a);
    }
    // Dernier recours : le nom annoncé (« Slack », « Zoom »…). Les apps Electron annoncent souvent
    // « Chromium » : on ne le tente qu'après l'exécutable et la descendance, et jamais pour un
    // navigateur (un nom « Chromium » ne prouve pas que le son vient du navigateur).
    client
        .app_name
        .as_ref()
        .and_then(|n| WatchedApp::best_match(n, apps))
        .map(|b| b.0)
        .filter(|a| crate::titles::kind(a) != crate::titles::Kind::Browser)
}

pub fn build_snapshot(clients: &[AudioClient], procs: &HashMap<u32, ProcInfo>, apps: &[WatchedApp], self_pid: u32) -> CallSnapshot {
    let mut streams: HashMap<String, Vec<PlaybackStream>> = HashMap::new();
    let mut input: HashSet<String> = HashSet::new();
    for c in clients {
        if c.pid == Some(self_pid) {
            continue; // Sténo lui-même n'est jamais une app de visio.
        }
        let Some(app) = attribute(c, procs, apps) else { continue };
        match c.kind {
            ClientKind::Playback => {
                if let Some(sink) = c.sink {
                    streams.entry(app.id.clone()).or_default().push((c.index, sink));
                }
            }
            ClientKind::Record => {
                input.insert(app.id.clone());
            }
        }
    }
    let mut pids: HashMap<String, Vec<u32>> = HashMap::new();
    for p in procs.values() {
        if p.pid == self_pid {
            continue;
        }
        if let Some(a) = match_names(&p.names, apps) {
            pids.entry(a.id.clone()).or_default().push(p.pid);
        }
    }
    CallSnapshot {
        apps: apps
            .iter()
            .map(|app| {
                let mut s = streams.remove(&app.id).unwrap_or_default();
                s.sort();
                let mut p = pids.remove(&app.id).unwrap_or_default();
                p.sort();
                AppSnapshot { app: app.clone(), streams: s, pids: p, input_active: input.contains(&app.id) }
            })
            .collect(),
    }
}

/// Relevé instantané.
pub fn scan(pulse: &Pulse, apps: &[WatchedApp]) -> CallSnapshot {
    if apps.is_empty() {
        return CallSnapshot::default();
    }
    let clients = pulse.clients().unwrap_or_else(|e| {
        crate::log::debug(&format!("flux audio illisibles : {e:#}"));
        vec![]
    });
    let procs: HashMap<u32, ProcInfo> = procinfo::all().into_iter().map(|p| (p.pid, p)).collect();
    build_snapshot(&clients, &procs, apps, std::process::id())
}

// MARK: - Moniteur

pub enum MonitorEvent {
    Call { app: WatchedApp, active: bool, title: Option<String> },
    Snapshot(CallSnapshot),
}

enum Cmd {
    SetApps(Vec<WatchedApp>),
    ScanNow(Sender<CallSnapshot>),
    Poke,
    Stop,
}

#[derive(Clone)]
pub struct CallMonitor {
    tx: Sender<Cmd>,
}

#[derive(Default)]
struct Tracking {
    activity: CallActivity,
    title: Option<String>,
}

impl CallMonitor {
    pub fn start(pulse: Pulse, events: Sender<MonitorEvent>) -> CallMonitor {
        let (tx, rx) = unbounded();
        std::thread::Builder::new().name("steno.calls".into()).spawn(move || run(pulse, rx, events)).expect("thread des appels");
        CallMonitor { tx }
    }

    /// Remplace la liste surveillée ; un appel en cours dans une app retirée est déclaré terminé.
    pub fn set_apps(&self, apps: Vec<WatchedApp>) {
        let _ = self.tx.send(Cmd::SetApps(apps));
    }

    /// Relevé immédiat avec la liste courante, sans toucher à l'état des appels.
    pub fn scan_now(&self) -> CallSnapshot {
        let (tx, rx) = bounded(1);
        if self.tx.send(Cmd::ScanNow(tx)).is_err() {
            return CallSnapshot::default();
        }
        rx.recv_timeout(Duration::from_secs(8)).unwrap_or_default()
    }

    /// Relevé anticipé (un flux audio vient d'apparaître ou de disparaître).
    pub fn poke(&self) {
        let _ = self.tx.send(Cmd::Poke);
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

fn read_title(x11: &mut Option<crate::x11::X11>, x11_retry: &mut std::time::Instant, app: &AppSnapshot) -> Option<String> {
    if x11.is_none() && std::time::Instant::now() >= *x11_retry {
        *x11 = crate::x11::X11::connect();
        *x11_retry = std::time::Instant::now() + Duration::from_secs(30);
    }
    let conn = x11.as_ref()?;
    let windows = conn.windows(&app.pids.iter().copied().collect());
    if windows.is_empty() {
        return None;
    }
    crate::titles::pick_title(&app.app, &windows)
}

fn run(pulse: Pulse, rx: Receiver<Cmd>, events: Sender<MonitorEvent>) {
    let mut apps = WatchedApp::defaults();
    let mut tracking: HashMap<String, Tracking> = HashMap::new();
    let mut x11: Option<crate::x11::X11> = None;
    let mut x11_retry = std::time::Instant::now();
    let interval = Duration::from_secs(2);
    let mut next = std::time::Instant::now();
    let mut last_poll = std::time::Instant::now() - Duration::from_secs(1);
    loop {
        let wait = next.saturating_duration_since(std::time::Instant::now());
        let poll_now: bool;
        match rx.recv_timeout(wait) {
            Ok(Cmd::Stop) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
            Ok(Cmd::SetApps(new)) => {
                let ids: HashSet<&str> = new.iter().map(|a| a.id.as_str()).collect();
                for app in &apps {
                    if !ids.contains(app.id.as_str()) {
                        if tracking.get(&app.id).map(|t| t.activity.active).unwrap_or(false) {
                            crate::log::info(&format!("{} n'est plus surveillé pendant un appel : fin de l'appel annoncée", app.name));
                            let _ = events.send(MonitorEvent::Call { app: app.clone(), active: false, title: None });
                        }
                        tracking.remove(&app.id);
                    }
                }
                apps = new;
                poll_now = true;
            }
            Ok(Cmd::ScanNow(reply)) => {
                let _ = reply.send(scan(&pulse, &apps));
                continue;
            }
            Ok(Cmd::Poke) => {
                // Une rafale d'événements (flux créé puis propriétés publiées) : un seul relevé
                // par tranche de 300 ms, le relevé périodique rattrape le reste.
                if last_poll.elapsed() < Duration::from_millis(300) {
                    next = next.min(last_poll + Duration::from_millis(300));
                    continue;
                }
                std::thread::sleep(Duration::from_millis(50));
                poll_now = true;
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => poll_now = true,
        }
        if !poll_now {
            continue;
        }
        next = std::time::Instant::now() + interval;
        last_poll = std::time::Instant::now();
        let snapshot = scan(&pulse, &apps);
        for entry in &snapshot.apps {
            let t = tracking.entry(entry.app.id.clone()).or_default();
            match t.activity.update(entry.input_active) {
                Transition::Started => {
                    t.title = read_title(&mut x11, &mut x11_retry, entry);
                    crate::log::info(&format!("appel {} détecté (titre : {})", entry.app.name, t.title.as_deref().unwrap_or("inconnu")));
                    let _ = events.send(MonitorEvent::Call { app: entry.app.clone(), active: true, title: t.title.clone() });
                }
                Transition::Ended => {
                    t.title = None;
                    crate::log::info(&format!("fin de l'appel {}", entry.app.name));
                    let _ = events.send(MonitorEvent::Call { app: entry.app.clone(), active: false, title: None });
                }
                Transition::None => {
                    // Le titre est figé dès qu'on en trouve un plausible.
                    if t.activity.active && t.title.is_none() {
                        if let Some(title) = read_title(&mut x11, &mut x11_retry, entry) {
                            t.title = Some(title.clone());
                            let _ = events.send(MonitorEvent::Call { app: entry.app.clone(), active: true, title: Some(title) });
                        }
                    }
                }
            }
        }
        let _ = events.send(MonitorEvent::Snapshot(snapshot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(kind: ClientKind, index: u32, pid: u32, binary: &str, name: &str) -> AudioClient {
        AudioClient { kind, index, pid: Some(pid), binary: Some(binary.into()).filter(|b: &String| !b.is_empty()), app_name: Some(name.into()), app_ids: vec![], sink: Some(0), corked: false }
    }
    fn proc_(pid: u32, ppid: u32, names: &[&str]) -> (u32, ProcInfo) {
        (pid, ProcInfo { pid, ppid: Some(ppid), names: names.iter().map(|n| n.to_string()).collect() })
    }

    #[test]
    fn anti_rebond() {
        let mut a = CallActivity::default();
        assert_eq!(a.update(true), Transition::Started);
        assert_eq!(a.update(true), Transition::None);
        assert_eq!(a.update(false), Transition::None);
        assert_eq!(a.update(true), Transition::None);
        assert_eq!(a.update(false), Transition::None);
        assert_eq!(a.update(false), Transition::Ended);
        assert_eq!(a.update(false), Transition::None);
    }

    #[test]
    fn selection_des_apps_captees() {
        let w = WatchedApp::catalog();
        let (apps, unknown) = resolve_capture(&["slack".into(), "inconnue".into()], &w);
        assert_eq!(apps.len(), 1);
        assert_eq!(unknown, vec!["inconnue"]);
        assert_eq!(resolve_capture(&[], &w).0.len(), w.len());
        assert_eq!(resolve_capture(&["x".into()], &w).0.len(), w.len());
    }

    #[test]
    fn attribution_et_releve() {
        let apps = WatchedApp::catalog();
        let procs: HashMap<u32, ProcInfo> = [
            proc_(100, 1, &["slack"]),
            proc_(101, 100, &["slack"]),
            proc_(200, 1, &["chrome"]),
            proc_(201, 200, &["chrome"]),
            proc_(300, 1, &["electron", "Web Content"]),
            proc_(301, 1, &["teams-for-linux"]),
            proc_(302, 301, &["electron-helper"]),
            proc_(999, 1, &["steno-audio"]),
        ]
        .into_iter()
        .collect();
        let clients = vec![
            // Flux de Slack venant du moteur de rendu, exécutable annoncé « slack ».
            client(ClientKind::Playback, 10, 101, "slack", "Chromium"),
            client(ClientKind::Record, 11, 101, "slack", "Chromium"),
            // Auxiliaire de Teams sans nom reconnaissable : rattaché par son parent.
            client(ClientKind::Playback, 12, 302, "", "Chromium"),
            // Processus inconnu.
            client(ClientKind::Playback, 13, 300, "electron", "Chromium"),
            // Sténo lui-même.
            client(ClientKind::Record, 14, 999, "slack", "Sténo"),
        ];
        let snap = build_snapshot(&clients, &procs, &apps, 999);
        let slack = snap.apps.iter().find(|a| a.app.id == "slack").unwrap();
        assert_eq!(slack.streams, vec![(10, 0)]);
        assert!(slack.input_active);
        assert_eq!(slack.pids, vec![100, 101]);
        let teams = snap.apps.iter().find(|a| a.app.id == "teams").unwrap();
        assert_eq!(teams.streams, vec![(12, 0)]);
        assert!(!teams.input_active);
        assert_eq!(snap.streams(&["slack".into(), "teams".into()]), vec![(10, 0), (12, 0)]);
        let meet = snap.apps.iter().find(|a| a.app.id == "meet").unwrap();
        assert!(meet.streams.is_empty());
        assert_eq!(meet.pids, vec![200, 201]);
    }
}
