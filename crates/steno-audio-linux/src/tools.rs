//! Modes en ligne de commande (diagnostic) : périphériques, flux audio des apps, transcription
//! d'un fichier, enregistrement des deux canaux.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::calls;
use crate::protocol::WatchedApp;
use crate::pulse::{Pulse, PulseEvent, RecordSource};

fn connect() -> Option<Pulse> {
    let (pulse, events) = Pulse::start();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(PulseEvent::Connected) = events.recv_timeout(Duration::from_millis(100)) {
            return Some(pulse);
        }
    }
    eprintln!("Serveur son injoignable : PulseAudio, ou PipeWire avec pipewire-pulse, doit tourner dans la session.");
    None
}

pub fn list_devices() -> i32 {
    let Some(pulse) = connect() else { return 1 };
    match pulse.devices() {
        Ok(d) => {
            println!("Micros (input_device / set_input) :");
            for i in &d.inputs {
                println!("  {} {:<40} {:<10} {}", if i.is_default { "*" } else { " " }, i.name, i.transport, i.id);
            }
            println!("Sorties (play.device) :");
            for o in &d.outputs {
                println!("  {} {:<40} {:<10} {}", if o.is_default { "*" } else { " " }, o.name, o.transport, o.id);
            }
            0
        }
        Err(e) => {
            eprintln!("ERREUR : {e:#}");
            1
        }
    }
}

pub fn list_audio_processes(apps: &[WatchedApp]) -> i32 {
    let Some(pulse) = connect() else { return 1 };
    let clients = match pulse.clients() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ERREUR : {e:#}");
            return 1;
        }
    };
    let procs: HashMap<u32, crate::procinfo::ProcInfo> = crate::procinfo::all().into_iter().map(|p| (p.pid, p)).collect();
    println!("{:<8} {:<6} {:<8} {:<24} {:<24} {:<10} app", "type", "index", "pid", "exécutable", "nom", "état");
    for c in &clients {
        let app = calls::attribute(c, &procs, apps).map(|a| a.id.clone()).unwrap_or_else(|| "-".into());
        println!(
            "{:<8} {:<6} {:<8} {:<24} {:<24} {:<10} {}",
            if c.kind == crate::pulse::ClientKind::Playback { "lecture" } else { "micro" },
            c.index,
            c.pid.map(|p| p.to_string()).unwrap_or_else(|| "?".into()),
            c.binary.clone().unwrap_or_default(),
            c.app_name.clone().unwrap_or_default(),
            if c.corked { "en pause" } else { "actif" },
            app
        );
    }
    let snap = calls::build_snapshot(&clients, &procs, apps, std::process::id());
    for a in &snap.apps {
        println!("{} : {} flux de lecture, micro {}, {} processus", a.app.name, a.streams.len(), if a.input_active { "utilisé (appel)" } else { "inutilisé" }, a.pids.len());
    }
    0
}

pub fn transcribe_file(path: &Path) -> i32 {
    let samples = match crate::wav::load_mono_16k(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ERREUR : {e:#}");
            return 1;
        }
    };
    let t = Instant::now();
    let r = match crate::models::Recognizer::load() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ERREUR : {e:#}");
            return 1;
        }
    };
    println!("Parakeet v3 chargé en {:.1} s", t.elapsed().as_secs_f64());
    // Découpage comme en direct : VAD Silero + segmenteur.
    let mut vad = crate::models::ChannelVad::make();
    let mut seg = crate::segmenter::UtteranceSegmenter::new(Default::default());
    let mut utterances = vec![];
    for frame in samples.chunks(crate::models::VAD_FRAME) {
        let mut f = frame.to_vec();
        f.resize(crate::models::VAD_FRAME, 0.0);
        let p = vad.probability(&f);
        utterances.extend(seg.push(&f, p));
    }
    utterances.extend(seg.flush());
    let mut total = 0.0;
    for u in &utterances {
        let s = Instant::now();
        let text = r.transcribe(&u.samples).unwrap_or_default();
        let el = s.elapsed().as_secs_f64();
        total += el;
        println!("[{:>7.2} – {:>7.2}] ({:.2} s) {}", u.t0(), u.t1(), el, text);
    }
    let dur = samples.len() as f64 / 16_000.0;
    println!("{} phrases, {:.1} s d'audio transcrits en {:.1} s (facteur temps réel {:.2})", utterances.len(), dur, total, total / dur.max(0.001));
    0
}

pub fn record(seconds: f64, dir: &Path, apps: &[WatchedApp]) -> i32 {
    let Some(pulse) = connect() else { return 1 };
    let snapshot = calls::scan(&pulse, apps);
    let ids: Vec<String> = apps.iter().map(|a| a.id.clone()).collect();
    let streams = snapshot.streams(&ids);
    let devices = pulse.devices().unwrap_or_default();
    let me = Arc::new(Mutex::new(Vec::<f32>::new()));
    let them = Arc::new(Mutex::new(Vec::<f32>::new()));
    let m = me.clone();
    let mut opened = vec![];
    match pulse.open(RecordSource::Source(None), Box::new(move |s| m.lock().unwrap().extend_from_slice(&s))) {
        Ok((id, name)) => {
            println!("micro : {name}");
            opened.push(id);
        }
        Err(e) => eprintln!("micro : {e:#}"),
    }
    if streams.is_empty() {
        println!("aucun flux audio de {} : seul le micro est enregistré", apps.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", "));
    }
    for (index, sink) in streams {
        let Some(monitor) = devices.sinks.iter().find(|s| s.index == sink).map(|s| s.monitor_source.clone()) else { continue };
        let t = them.clone();
        match pulse.open(RecordSource::SinkInput { index, monitor_source: monitor }, Box::new(move |s| t.lock().unwrap().extend_from_slice(&s))) {
            Ok((id, _)) => {
                println!("flux d'app {index} capté");
                opened.push(id);
            }
            Err(e) => eprintln!("flux {index} : {e:#}"),
        }
    }
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < seconds {
        std::thread::sleep(Duration::from_secs(1));
        let lv = |b: &Arc<Mutex<Vec<f32>>>| {
            let v = b.lock().unwrap();
            let tail = &v[v.len().saturating_sub(16_000)..];
            crate::levels::dbfs(tail)
        };
        println!("{:>4.0} s  moi {:>6.1} dBFS  eux {:>6.1} dBFS", start.elapsed().as_secs_f64(), lv(&me), lv(&them));
    }
    for id in opened {
        pulse.close(id);
    }
    std::thread::sleep(Duration::from_millis(200));
    for (name, buf) in [("me.wav", &me), ("them.wav", &them)] {
        let path = dir.join(name);
        match crate::wav::WavWriter::create(&path, 16_000).and_then(|mut w| {
            w.append(&buf.lock().unwrap())?;
            w.close()
        }) {
            Ok(()) => println!("{}", path.display()),
            Err(e) => eprintln!("{name} : {e:#}"),
        }
    }
    0
}
