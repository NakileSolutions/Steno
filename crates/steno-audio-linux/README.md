# steno-audio (Linux) — Sténo's audio sidecar

Rust binary launched by the Sténo core on Linux, the counterpart of the macOS Swift sidecar
(`audio/StenoAudio`). Same **JSON lines** protocol over stdin/stdout (`docs/contracts.md` §2),
logs **only on stderr**. No audio ever leaves the computer; only the models are downloaded
(once).

| Role | macOS (Swift) | Linux (this crate) |
|---|---|---|
| Microphone (`me` channel) | AVAudioEngine + voice processing | PulseAudio / PipeWire recording stream + WebRTC AEC3 echo cancellation (`aec.rs`) |
| Call audio (`them` channel) | Core Audio process tap on the app's processes | monitor of **each playback stream** of the app (`pa_stream_set_monitor_stream`), mixed together (`capture.rs`) |
| Call detection | an app process using audio input | a recording stream from a real microphone (not a monitor) opened by the app (`calls.rs`) |
| Process ownership | bundle ID prefix, otherwise parent PID | executable name prefix, Flatpak / Snap ID, otherwise parent PID, otherwise `application.name` (never for a browser) |
| Call title | accessibility | X11 / XWayland windows (`x11.rs`), same heuristics (`titles.rs`) |
| VAD, ASR | Silero, Parakeet TDT v3 (FluidAudio, Neural Engine) | Silero, Parakeet TDT v3 int8 (sherpa-onnx, ONNX Runtime on CPU) |
| Re-transcription | Canary 1B v2 | Canary 180M Flash (fr, en, es, de) |
| Diarization | FluidAudio | pyannote segmentation 3.0 + 3D-Speaker ERes2Net (sherpa-onnx) |
| Devices | Core Audio UID | PulseAudio node name (`alsa_input.usb-…`), stable across reconnections |
| Calendar | EventKit | Evolution Data Server (local cache), `.ics` files from `calendars.txt` (`calendar.rs`, `ical.rs`) |
| Permissions | TCC | none: `microphone` / `system_audio` = `granted` as soon as the sound server responds, `accessibility` = `granted` if X11 is reachable, `calendar` = `granted` if a source exists |

The system-independent parts are faithful ports of the Swift code, with their tests: protocol,
segmenter, false-sentence filter, vocabulary correction, re-transcription, diarization
formatting, title heuristics, device sorting, sound cue.

Measured performance (4 x86_64 cores): Parakeet transcribes about ten times faster than real
time (5 s of audio in 0.4 s); model loaded in 2 s.

## Build

```sh
make deps      # apt packages (once)
make sidecar   # downloads sherpa-onnx (curl), builds, copies into app/src-tauri/binaries/
```

By hand: `SHERPA_ONNX_ARCHIVE_DIR=<archive-dir> cargo build --release -p steno-audio-linux`
(without this variable, the `sherpa-onnx-sys` build script downloads the archive itself). The
`aec` feature (on by default) builds WebRTC with meson and ninja.

On macOS, this crate builds as an empty binary (the Swift sidecar is used instead):
`cargo test --workspace` works everywhere.

## Models

Directory: `STENO_MODELS_DIR`, otherwise `$STENO_HOME/models`, otherwise `~/.local/share/Steno/models`.
`steno-audio --prefetch-models` (run by `make install`) downloads everything (≈ 700 MB) from the
sherpa-onnx releases; otherwise each model is downloaded on first use.

## Options and variables

```
steno-audio                         protocol mode
steno-audio --list-devices          microphones and outputs with their identifier
steno-audio --list-audio-processes [--apps teams,slack]
                                    application audio streams and the app recognized for each
steno-audio --spike-record <s> <dir> [--apps …]   records me.wav and them.wav
steno-audio --spike-transcribe <f.wav>                transcribes a file (VAD + Parakeet)
steno-audio --prefetch-models
```

| Variable | Effect |
|---|---|
| `STENO_TAP_MODE=global` | plan B: all audio from the default output instead of the apps' streams |
| `STENO_AEC=0` / `STENO_AEC=1` | echo cancellation disabled / forced (default: enabled only when the default output is not a headset, re-evaluated on every output change) |
| `STENO_VAD=energy` | energy-based VAD instead of Silero |
| `STENO_MODELS_DIR` | models directory |
| `STENO_PRELOAD_MODELS=1` | loads Parakeet at startup |
| `STENO_EMAIL` | the user's address(es), used to identify "me" among the invitees |
| `STENO_AUDIO_DEBUG=1` | verbose logs |

## Tests performed (PulseAudio server, virtual outputs and microphone)

- simulated call (fake `slack` process that records the microphone and plays the participants'
  voices): call detected, only the app's stream captured, sentences transcribed on both channels,
  `stop`, diarization (two voices told apart), Canary re-transcription, `wake_test`, `shutdown`;
- speaker echo fed back into the microphone at −9 dB: without AEC the participants' voices are
  transcribed on the `me` channel; with AEC they disappear, and the user's speech is still
  transcribed even while the participants are talking;
- without echo (headset): AEC3 muffled the beginning of sentences spoken while the participants
  were talking, until it converged; hence automatic activation on speakers only (headset output:
  "headphones" jack, Bluetooth, `device.form_factor` headset/headphone);
- selected microphone unplugged (`device_missing`, fallback to the default) then plugged back in
  (resumed), `set_input`, sound server restart while listening (reconnection, streams reopened);
- full chain `steno serve` → core → this sidecar → meeting notes (fake agent).

Not tested for lack of hardware: real Teams for Linux / Slack / Zoom clients, native PipeWire
(only PulseAudio was used; PipeWire serves the same API through pipewire-pulse), Bluetooth headsets.
