//! steno-audio (Linux) — sidecar audio de Sténo : capture du micro et du son des apps de visio
//! (PulseAudio / PipeWire), VAD, transcription Parakeet (sherpa-onnx), diarisation et
//! retranscription de fin de réunion. Même protocole JSON lines que le sidecar Swift de macOS
//! (docs/contracts.md §2). Journaux sur stderr uniquement.

// Logique pure, testée sur toutes les plateformes.
mod filters;
mod levels;
mod post;
mod protocol;
mod resample;
mod segmenter;
mod text;
mod titles;
#[cfg(unix)]
mod wav;

// Capture, modèles et fenêtres : Linux.
#[cfg(target_os = "linux")]
mod aec;
#[cfg(target_os = "linux")]
mod calendar;
#[cfg(target_os = "linux")]
mod calls;
#[cfg(target_os = "linux")]
mod capture;
#[cfg(target_os = "linux")]
mod controller;
#[cfg(target_os = "linux")]
mod devices;
#[cfg(target_os = "linux")]
mod log;
#[cfg(target_os = "linux")]
mod models;
#[cfg(target_os = "linux")]
mod pipeline;
#[cfg(target_os = "linux")]
mod procinfo;
#[cfg(target_os = "linux")]
mod pulse;
#[cfg(target_os = "linux")]
mod x11;

#[cfg(not(target_os = "linux"))]
mod log {
    pub fn warn(m: &str) {
        eprintln!("[steno-audio] WARN {m}");
    }
    pub fn info(m: &str) {
        eprintln!("[steno-audio] INFO {m}");
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("steno-audio (Linux) : sur macOS, utilisez le sidecar Swift (audio/StenoAudio).");
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
fn main() {
    linux::main()
}

#[cfg(target_os = "linux")]
mod linux {
    use std::io::BufRead;
    use std::path::PathBuf;

    use crate::protocol::{self, WatchedApp};

    const USAGE: &str = "steno-audio {VERSION} (Linux) — sidecar audio de Sténo

Usage :
  steno-audio                                   mode protocole (JSON lines sur stdin/stdout)
  steno-audio --spike-record <secondes> <dossier> [--apps teams,slack]
                                                 enregistre me.wav et them.wav (micro et son des apps)
  steno-audio --spike-transcribe <fichier.wav>  transcrit un fichier, temps par phrase
  steno-audio --list-audio-processes [--apps teams,slack]
                                                 liste les flux audio des applications et l'app reconnue
  steno-audio --list-devices                    liste les micros et sorties avec leur identifiant
                                                 (valeurs de input_device / set_input / play)
  steno-audio --prefetch-models                 télécharge et prépare les modèles (make install)
  steno-audio --version

Apps connues de --apps : {APPS} (par défaut : teams).
En mode protocole, la liste des apps vient du cœur (commande watch).

Variables d'environnement :
  STENO_TAP_MODE=global   capte tout le son de la sortie par défaut au lieu des apps de visio
  STENO_VAD=energy        VAD par énergie au lieu de Silero
  STENO_AEC=0             sans annulation d'écho sur le micro
  STENO_MODELS_DIR=<dir>  dossier des modèles (sinon $STENO_HOME/models ou ~/.local/share/Steno/models)
  STENO_PRELOAD_MODELS=1  charge le modèle de transcription dès le lancement
  STENO_AUDIO_DEBUG=1     journaux détaillés sur stderr";

    fn usage() -> String {
        USAGE.replace("{VERSION}", protocol::VERSION).replace("{APPS}", &WatchedApp::catalog().iter().map(|a| a.id.clone()).collect::<Vec<_>>().join(", "))
    }

    extern "C" fn on_signal(_: libc::c_int) {
        crate::controller::SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn apps_option(rest: &[String]) -> Vec<WatchedApp> {
        if rest.is_empty() {
            return WatchedApp::defaults();
        }
        if rest.len() != 2 || rest[0] != "--apps" {
            eprintln!("Arguments inattendus : {}\n\n{}", rest.join(" "), usage());
            std::process::exit(64);
        }
        let (apps, unknown) = WatchedApp::from_catalog(&rest[1]);
        if !unknown.is_empty() {
            eprintln!("App(s) inconnue(s) : {}. Apps connues : {}.", unknown.join(", "), WatchedApp::catalog().iter().map(|a| a.id.clone()).collect::<Vec<_>>().join(", "));
            std::process::exit(64);
        }
        apps
    }

    fn expand(p: &str) -> PathBuf {
        match p.strip_prefix("~/") {
            Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
            None => PathBuf::from(p),
        }
    }

    pub fn main() {
        unsafe {
            // Si le cœur disparaît, write(2) renvoie EPIPE au lieu de tuer le processus.
            libc::signal(libc::SIGPIPE, libc::SIG_IGN);
            libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
            libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        }
        let args: Vec<String> = std::env::args().skip(1).collect();
        let code = match args.first().map(String::as_str) {
            None => run_protocol(),
            Some("--version") => {
                println!("{}", protocol::VERSION);
                0
            }
            Some("--help") | Some("-h") => {
                println!("{}", usage());
                0
            }
            Some("--list-devices") => crate::tools::list_devices(),
            Some("--list-audio-processes") => crate::tools::list_audio_processes(&apps_option(&args[1..])),
            Some("--prefetch-models") => match crate::models::prefetch() {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("ERREUR : {e:#}");
                    1
                }
            },
            Some("--spike-transcribe") if args.len() == 2 => crate::tools::transcribe_file(&expand(&args[1])),
            Some("--spike-record") if args.len() >= 3 => match args[1].parse::<f64>() {
                Ok(secs) if secs > 0.0 => crate::tools::record(secs, &expand(&args[2]), &apps_option(&args[3..])),
                _ => {
                    eprintln!("{}", usage());
                    64
                }
            },
            Some(other) => {
                eprintln!("Argument inconnu : {other}\n\n{}", usage());
                64
            }
        };
        std::process::exit(code);
    }

    fn run_protocol() -> i32 {
        let controller = crate::controller::Controller::new();
        let (tx, rx) = crossbeam_channel::unbounded::<Option<String>>();
        std::thread::Builder::new()
            .name("steno.stdin".into())
            .spawn(move || {
                let stdin = std::io::stdin();
                for line in stdin.lock().lines() {
                    let Ok(line) = line else { break };
                    let t = line.trim();
                    if !t.is_empty() && tx.send(Some(t.to_string())).is_err() {
                        return;
                    }
                }
                let _ = tx.send(None);
            })
            .expect("thread stdin");
        controller.run(rx)
    }
}

#[cfg(target_os = "linux")]
mod tools;
