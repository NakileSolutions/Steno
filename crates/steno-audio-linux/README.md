# steno-audio (Linux) — sidecar audio de Sténo

Binaire Rust lancé par le cœur de Sténo sous Linux, équivalent du sidecar Swift de macOS
(`audio/StenoAudio`). Même protocole **JSON lines** sur stdin/stdout (`docs/contracts.md` §2),
journaux **uniquement sur stderr**. Aucun audio ne quitte l'ordinateur ; seuls les modèles sont
téléchargés (une fois).

| Rôle | macOS (Swift) | Linux (ce crate) |
|---|---|---|
| Micro (canal `me`) | AVAudioEngine + voice processing | flux d'enregistrement PulseAudio / PipeWire + annulation d'écho WebRTC AEC3 (`aec.rs`) |
| Son de l'appel (canal `them`) | process tap Core Audio sur les processus de l'app | moniteur de **chaque flux de lecture** de l'app (`pa_stream_set_monitor_stream`), mélangés (`capture.rs`) |
| Détection d'appel | processus de l'app qui utilise l'entrée audio | flux d'enregistrement d'un vrai micro ouvert par l'app (pas un moniteur) (`calls.rs`) |
| Appartenance d'un processus | préfixe de bundle ID, sinon PID parent | préfixe du nom d'exécutable, de l'identifiant Flatpak / Snap, sinon PID parent, sinon `application.name` (jamais pour un navigateur) |
| Titre de l'appel | accessibilité | fenêtres X11 / XWayland (`x11.rs`), mêmes heuristiques (`titles.rs`) |
| VAD, ASR | Silero, Parakeet TDT v3 (FluidAudio, Neural Engine) | Silero, Parakeet TDT v3 int8 (sherpa-onnx, ONNX Runtime sur CPU) |
| Retranscription | Canary 1B v2 | Canary 180M Flash (fr, en, es, de) |
| Diarisation | FluidAudio | pyannote segmentation 3.0 + 3D-Speaker ERes2Net (sherpa-onnx) |
| Périphériques | UID Core Audio | nom du nœud PulseAudio (`alsa_input.usb-…`), stable d'un branchement à l'autre |
| Calendrier | EventKit | Evolution Data Server (cache local), `.ics` de `calendars.txt` (`calendar.rs`, `ical.rs`) |
| Permissions | TCC | aucune : `microphone` / `system_audio` = `granted` dès que le serveur son répond, `accessibility` = `granted` si X11 est joignable, `calendar` = `granted` si une source existe |

Les parties sans dépendance au système sont des ports fidèles du Swift, avec leurs tests :
protocole, segmenteur, filtre des fausses phrases, correction par le vocabulaire, retranscription,
mise en forme de la diarisation, heuristiques de titre, tri des périphériques, signal sonore.

Performance mesurée (4 cœurs x86_64) : Parakeet transcrit environ dix fois plus vite que le temps
réel (5 s d'audio en 0,4 s) ; modèle chargé en 2 s.

## Compilation

```sh
make deps      # paquets apt (une fois)
make sidecar   # télécharge sherpa-onnx (curl), compile, copie dans app/src-tauri/binaries/
```

À la main : `SHERPA_ONNX_ARCHIVE_DIR=<dossier de l'archive> cargo build --release -p steno-audio-linux`
(sans cette variable, le script de compilation de `sherpa-onnx-sys` télécharge lui-même
l'archive). La fonction `aec` (par défaut) compile WebRTC avec meson et ninja.

Sur macOS, ce crate compile en binaire vide (le sidecar Swift sert) : `cargo test --workspace`
reste utilisable partout.

## Modèles

Dossier : `STENO_MODELS_DIR`, sinon `$STENO_HOME/models`, sinon `~/.local/share/Steno/models`.
`steno-audio --prefetch-models` (lancé par `make install`) télécharge tout (≈ 700 Mo) depuis les
publications de sherpa-onnx ; sinon chaque modèle est téléchargé à son premier usage.

## Options et variables

```
steno-audio                         mode protocole
steno-audio --list-devices          micros et sorties avec leur identifiant
steno-audio --list-audio-processes [--apps teams,slack]
                                    flux audio des applications et app reconnue pour chacun
steno-audio --spike-record <s> <dossier> [--apps …]   enregistre me.wav et them.wav
steno-audio --spike-transcribe <f.wav>                transcrit un fichier (VAD + Parakeet)
steno-audio --prefetch-models
```

| Variable | Effet |
|---|---|
| `STENO_TAP_MODE=global` | plan B : tout le son de la sortie par défaut au lieu des flux des apps |
| `STENO_AEC=0` / `STENO_AEC=1` | annulation d'écho désactivée / forcée (par défaut : active seulement quand la sortie par défaut n'est pas un casque, réévalué à chaque changement de sortie) |
| `STENO_VAD=energy` | VAD par énergie au lieu de Silero |
| `STENO_MODELS_DIR` | dossier des modèles |
| `STENO_PRELOAD_MODELS=1` | charge Parakeet dès le lancement |
| `STENO_EMAIL` | adresse(s) de l'utilisateur pour repérer « moi » parmi les invités |
| `STENO_AUDIO_DEBUG=1` | journaux détaillés |

## Essais réalisés (serveur PulseAudio, sorties et micro virtuels)

- appel simulé (faux processus `slack` qui enregistre le micro et joue la voix des participants) :
  appel détecté, flux de l'app capté seul, phrases transcrites dans les deux canaux, `stop`,
  diarisation (deux voix distinguées), retranscription Canary, `wake_test`, `shutdown` ;
- écho des haut-parleurs renvoyé dans le micro à −9 dB : sans AEC la voix des participants est
  transcrite sur le canal `me` ; avec AEC elle disparaît, et la parole de l'utilisateur reste
  transcrite même pendant que les participants parlent ;
- sans écho (casque) : AEC3 étouffait le début des phrases dites pendant que les participants
  parlent, le temps de converger ; d'où l'activation automatique seulement sur haut-parleurs
  (sortie casque : jack « headphones », Bluetooth, `device.form_factor` headset/headphone) ;
- micro choisi débranché (`device_missing`, repli sur le défaut) puis rebranché (repris),
  `set_input`, redémarrage du serveur son pendant l'écoute (reconnexion, flux rouverts) ;
- chaîne complète `steno serve` → cœur → ce sidecar → compte rendu (faux agent).

Non essayé faute de matériel : vrais clients Teams for Linux / Slack / Zoom, PipeWire natif
(seul PulseAudio a servi ; PipeWire sert la même API par pipewire-pulse), casques Bluetooth.
