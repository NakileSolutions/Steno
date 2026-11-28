# steno-audio — Sténo's audio sidecar

Swift binary launched by the Sténo core. It only turns audio into text:

- **microphone** capture (`me` channel) with `AVAudioEngine` and voice processing (echo cancellation);
- capture of **video-call app audio** (`them` channel) through a Core Audio *process tap* on the
  whole process tree of the captured apps: Teams, Slack, Zoom, browser… (macOS 14.2+);
- resampling to 16 kHz mono, **VAD** (Silero via FluidAudio, energy-based fallback), splitting into
  utterances (ending after `silence_ms` of silence or 25 s), **Parakeet TDT v3 transcription**
  on the Neural Engine, vocabulary correction;
- call detection in the watched video-call apps (list set by the core, `watch` command) and
  reading of the call title (accessibility);
- on-demand offline diarization of `them.wav`;
- listing of **audio devices** (microphones, outputs), microphone selection (at `start` or during
  capture) and a discreet detection sound cue on the chosen output.

It talks to the core in **JSON lines** over stdin/stdout, as specified in `docs/contracts.md` §2.
Logs go **only to stderr**. No data leaves the Mac (only the first launch downloads the models
from Hugging Face, via FluidAudio).

## Requirements

- macOS 14.2 or later, Apple Silicon;
- Xcode 16 (Swift 5.10 / 6.0) or later;
- network access on first launch (FluidAudio dependency and models).

## Build

```sh
cd audio/StenoAudio
swift build -c release            # binary: .build/release/steno-audio
swift test                        # hardware-free tests (protocol, vocabulary, segmenter…)
.build/release/steno-audio --prefetch-models   # downloads and compiles the models (≈ 1 GB)
```

`Package.swift` embeds `Info.plist` into the binary (`__TEXT,__info_plist` section) so that the
usage descriptions exist even when the sidecar is launched on its own.

Useful environment variables:

| Variable | Effect |
|---|---|
| `STENO_TAP_MODE=global` | plan B: tap on all of the Mac's audio except Sténo (if an app outputs audio through an unidentified process) |
| `STENO_VAD=energy` | energy-based VAD instead of Silero |
| `STENO_MODELS_DIR=<dir>` | models directory for all four models, one subfolder each (`parakeet-tdt-v3`, `silero-vad`, `diarizer`, `canary-1b-v2`); otherwise FluidAudio's default cache |
| `STENO_AUDIO_DEBUG=1` | verbose logs on stderr |

Other commands: `--version`, `--help`, `--list-devices` (microphones and outputs with their UID, as
the `devices` message presents them, then every HAL device with the reason it was excluded, if
any), `--list-audio-processes [--apps teams,slack]`
(diagnostic of the audio processes seen by Core Audio, with the app recognized for each: Teams
only by default), `--prefetch-models`, and the two spike modes described below. The `--apps`
option accepts the IDs of the built-in catalog: `teams`, `slack`, `zoom`, `meet` (browsers).

## Watched video-call apps

The core sets the list with `{"cmd":"watch","apps":[{"id","name","bundles":[préfixes]}]}`, right
after `ready` and then on every settings change. Before the first `watch`, only Teams is
watched (`com.microsoft.teams2`, `com.microsoft.teams`). An empty list stops watching anything.

- **Ownership**: a process belongs to an app if its bundle ID **starts with** one of the app's
  prefixes (case-insensitive; the longest prefix wins if two apps overlap), which covers helper
  processes (`com.microsoft.teams2.helper…`). An audio process without a recognized bundle belongs
  to the app it descends from (parent PID). Sténo itself is never counted.
- **Call**: every ~2 s, an app is "in a call" as soon as one of its processes uses audio input
  (`kAudioProcessPropertyIsRunningInput`); the call ends after two consecutive negative readings.
  State is **tracked per app**: each change emits `{"type":"call","active":…,"app":"<id>","title":…}`
  (several calls can be active at once). The old `teams_call` message is no longer emitted.
  Removing an app that is in a call from a `watch` emits its call end.
- **Title** (accessibility, `CallTitleHeuristics`): Teams, separate meeting window (original
  heuristic); Slack, huddle window (« Huddle… », « Appel… » with the channel), otherwise `null`;
  Zoom, « Zoom Meeting » / « Réunion Zoom » window (topic if it appears in the title); browsers,
  active « Meet – … » tab (« Google Meet » if only the meeting code is visible); other apps,
  title of the frontmost window, unless it is generic (app name, empty). The title is frozen as
  soon as a plausible one is found; `null` if accessibility is denied or nothing fits.
- **Capture**: `start.apps` (e.g. `["slack"]`) selects the apps whose audio goes to the `them`
  channel; absent or empty = all watched apps. Unknown IDs are ignored (logged); if none remain,
  all watched apps are captured. The tap covers the union of these apps' processes and is
  recreated when that list changes (restart, new helper process, `watch`). If no process matches
  the `start` (app not launched yet or with no open audio), the microphone starts anyway, a
  recoverable `tap_failed` `error` explains why, and the tap is created as soon as the processes
  appear.

## Devices

Code: `AudioDevices.swift` (enumeration, filtering, sorting, listeners), `FeedbackPlayer.swift`
(sound cue), `MicCapture.swift` (microphone selection), `CaptureSession` / `Controller` (commands).

- **List**: `{"type":"devices","inputs":[…],"outputs":[…]}`, each entry with `id` (Core Audio
  UID, `kAudioDevicePropertyDeviceUID`), `name`, `default` and `transport`
  (`builtin | usb | bluetooth | virtual | aggregate | other`, from
  `kAudioDevicePropertyTransportType`: Bluetooth and Bluetooth LE → `bluetooth`, manual and
  automatic aggregates → `aggregate`, PCI, HDMI, DisplayPort, AirPlay, Thunderbolt, Continuity… →
  `other`). Sorting: default device first, then by name (ignoring case and accents, numbers
  in natural order), then by UID.
- **Filtering**: `inputs` only keeps devices with at least one input channel, `outputs`
  those with at least one output channel; excluded are entries with an empty UID or name,
  duplicate UIDs and private aggregates: Sténo's tap aggregate (`steno-tap-…`, name `Steno-Tap`)
  and those AVAudioEngine creates in our process (`CADefaultDeviceAggregate…`,
  `VPAUAggregateAudioDevice…`). A user-created aggregate is still offered.
- **Emission**: after `ready` (and `permissions`), in response to `{"cmd":"devices"}`, and on every
  change of the list or of the default devices (HAL listeners on
  `kAudioHardwarePropertyDevices`, `…DefaultInputDevice`, `…DefaultOutputDevice`), after a
  300 ms debounce and only if the message changes (creating the tap aggregate modifies the HAL
  list without changing `devices`).
- **Microphone selection**: `start.input_device` (`null` = system default microphone, following its
  changes; field absent = last remembered choice, otherwise the default microphone). The chosen
  microphone is set on the AVAudioEngine input node's unit (`kAudioOutputUnitProperty_CurrentDevice`)
  **before** voice processing is enabled, then read back afterwards: if it was lost, it is set
  again (the log says so). If the UID cannot be found, or if the microphone is unplugged during
  capture, the default microphone takes over and the sidecar emits, once per absence,
  `{"type":"error","code":"device_missing","message":"Micro « <uid> » introuvable : micro par défaut utilisé.","recoverable":true}`.
  As soon as the microphone reappears in the device list, it is used again.
- **`set_input`**: `{"cmd":"set_input","device":"<uid>"|null}` switches microphone during capture,
  restarting only the microphone engine (the tap keeps running). The `t0/t1` clock continues; the
  short gap is filled as for a device change (silence in `me.wav`, segmenter clock advanced) and
  the resampler follows the new microphone's sample rate. Outside capture, the choice is
  remembered for the next `start` (including for `wake_test` outside a meeting). `device`
  is required (`bad_command` otherwise).
- **`play`**: `{"cmd":"play","sound":"detect","device":"<uid>"|null}` plays two soft notes (E5
  then A5, 55 + 65 ms ≈ 120 ms, peak ≈ −15 dBFS, 8 ms fades) on the requested output (`null` or
  output not found = default output). Separate `AVAudioEngine`, created on demand
  (`AVAudioPlayerNode` + synthesized PCM buffer), output set through
  `kAudioOutputUnitProperty_CurrentDevice` on `outputNode`, stopped after 3 s of inactivity. The
  sound comes from our process: it never goes through the call app and is not captured by the tap
  (which only targets the video-call apps' processes, or everything except Sténo in global mode).
  The command returns immediately. `sound` absent = `detect`; unknown sound = `bad_command`.

## Integration into Steno.app (Tauri `externalBin`)

Tauri expects the binary suffixed with the target triple:

```sh
cd audio/StenoAudio
swift build -c release --arch arm64
mkdir -p ../../app/src-tauri/binaries
cp .build/arm64-apple-macosx/release/steno-audio \
   ../../app/src-tauri/binaries/steno-audio-aarch64-apple-darwin
```

In `app/src-tauri/tauri.conf.json`:

```json
{ "bundle": { "externalBin": ["binaries/steno-audio"] } }
```

Tauri then copies the binary to `Steno.app/Contents/MacOS/steno-audio` and signs it with the app
(ad hoc signing is enough). On the Rust side, the core launches it with `tauri-plugin-shell`
(`app.shell().sidecar("steno-audio")`) or `std::process::Command`, writes commands to stdin
and reads stdout line by line. Closing stdin (or sending `{"cmd":"shutdown"}`) stops the sidecar
cleanly.

The `Info.plist` of **Steno.app** (`app/src-tauri/Info.plist`) must contain the same keys as
`audio/StenoAudio/Info.plist`: `NSMicrophoneUsageDescription` and `NSAudioCaptureUsageDescription`.

### TCC permissions: who holds them?

macOS attributes permission requests (microphone, system audio recording) to the **responsible
process**, i.e. the app that launched the sidecar. Permissions are therefore requested and
granted **to Steno.app**, with the texts from its `Info.plist`; the sidecar inherits them as long
as it is launched by the app. The parent process is responsible for them: the sidecar only
observes the state (`permissions`) and reports denials (`mic_denied`, `system_audio_denied`).

Practical consequences:

- when launched from a terminal (spikes, `cargo tauri dev`), **the terminal** (Terminal, iTerm…)
  receives the requests and must be authorized;
- accessibility (call title) is granted to Steno.app in Réglages Système ›
  Confidentialité et sécurité › Accessibilité (System Settings › Privacy & Security › Accessibility);
- **to verify in spike S1**: that the authorization granted to Steno.app does cover the sidecar.
  **Plan B** if it does not: do the capture in Rust (`objc2-core-audio` for the tap,
  `objc2-avf-audio` for the microphone) in the app's main process, and keep only VAD and
  transcription in the sidecar (the audio would then be sent to it through a pipe).

## Spikes to run on the Mac (phase 0)

### S1 — Capture (blocking)

Goal: two audible tracks during a real Teams call, with permissions valid for the sidecar.
Steps 1 to 7 target Teams; **S1-bis** repeats them with Slack and Zoom.

1. Build: `swift build -c release`.
2. Start a Teams call (meeting with a colleague, or Paramètres › Appareils › « Passer un appel
   test » (Settings › Devices › "Make a test call")). Speak and have the other participant speak.
3. Identify Teams' audio processes:
   `.build/release/steno-audio --list-audio-processes`
   → note the bundles marked `teams` in the « app » column and check that no other process (e.g.
   `com.apple.WebKit.GPU`, Edge WebView helper) plays the call audio without being identified.
4. From the terminal (the terminal must be authorized for the microphone and system audio):
   `.build/release/steno-audio --spike-record 60 ~/Desktop/s1`
   → the « moi » ("me") and « eux » ("them") levels are shown every second; at the end, open
   `~/Desktop/s1/me.wav` and `them.wav` in QuickTime: both tracks must be audible,
   separate, and the echo of « eux » almost absent from `me.wav` (AEC).
5. During recording, unplug then replug the headset: stderr must report
   « micro reparti » ("microphone restarted") within 5 s.
6. **Decisive criterion**: redo step 4 from an **empty Tauri app** that launches the sidecar with
   the same arguments (`sidecar("steno-audio").args(["--spike-record","60","/tmp/s1"])`), after
   **revoking** the terminal's authorization. Check that the system prompts name the app, that
   both WAVs are audible, and that System Settings lists the app (not `steno-audio`).
7. If `them.wav` is silent while Teams is playing sound: retry with
   `STENO_TAP_MODE=global` (global tap); if that works, the audio goes through an unidentified
   process (step 3) → note its bundle in `docs/spikes.md`.

### S1-ter — Devices: to redo (microphone change, AirPods)

Goal: check the device list, microphone selection, switching microphones while listening and
the fallback when AirPods are connected then disconnected.

1. `.build/release/steno-audio --list-devices` without then with the AirPods connected: note the UIDs
   of the built-in microphone and of the AirPods; check `transport` (`builtin`, `bluetooth`), the
   default, and that no private aggregate appears in the first two lists. Redo during a capture
   (`--spike-record` in another terminal): the `Steno-Tap` aggregate and AVAudioEngine's
   aggregates must be marked « agrégat privé » ("private aggregate") in the last list (otherwise,
   add their prefix to `AudioDeviceCatalog.privateAggregateUIDPrefixes`).
2. Protocol mode, with commands sent through a named pipe so you can act between two commands:

   ```sh
   mkfifo /tmp/steno-in
   .build/release/steno-audio < /tmp/steno-in > /tmp/steno-out.jsonl &
   exec 3>/tmp/steno-in            # keeps the pipe open
   tail -f /tmp/steno-out.jsonl    # in another terminal
   echo '{"cmd":"start","keep_audio":true,"audio_dir":"/tmp/s1-ter","input_device":"BuiltInMicrophoneDevice"}' >&3
   # speak for 20 s, then:
   echo '{"cmd":"set_input","device":"<AirPods UID>"}' >&3
   # speak for 20 s into the AirPods, then put them back in their case (disconnection), speak for 20 s,
   # take them out again, speak for 20 s, then:
   echo '{"cmd":"set_input","device":null}' >&3
   echo '{"cmd":"play","sound":"detect","device":null}' >&3
   echo '{"cmd":"play","sound":"detect","device":"<AirPods UID>"}' >&3
   echo '{"cmd":"stop"}' >&3; exec 3>&-
   ```

   Expected: `devices` right after `ready` / `permissions`, then on every connection / disconnection
   of the AirPods (a single message per change thanks to the debounce); stderr « micro démarré : « AirPods
   … » » ("microphone started") after `set_input` without the tap restarting; when the AirPods are
   put away, one `error` `device_missing` (only once) and capture continuing on the default
   microphone; when they come back, « micro « … » de nouveau disponible : repris » ("available
   again: resumed"). The log says whether the second device setting (after voice processing is
   enabled) was needed.
3. Open `/tmp/s1-ter/me.wav`: speech from all four periods is present, the microphone-switch
   gaps last less than a second and `me.wav` stays aligned with `them.wav` (same
   duration within ±0.5 s); `me` segments have continuous `t0/t1`.
4. `start` with a nonexistent UID (`"input_device":"inexistant"`) → `device_missing` with that text,
   capture on the default microphone. `set_input` outside capture then `start` without `input_device` →
   the remembered microphone is used (log « capture démarrée (micro « … »…) » ("capture started")).
5. `play` cue: short, discreet, heard on the requested output (AirPods) or the default one; it
   must **not** be heard by the other participant of a Teams call, nor appear in
   `them.wav`. Also check that it is not attenuated by the microphone's voice processing.
6. During a call with the AirPods as microphone and output: AEC still active (`AEC actif` in the
   log) and call audio not lowered.

### S1-bis — Slack, Zoom (and Meet): to redo

Goal: check call detection, title and capture for each app, with the bundle prefixes the core
will send in `watch`.

1. For each app (Slack huddle in a channel, Zoom meeting, Google Meet in Chrome and Safari):
   during the call, `.build/release/steno-audio --list-audio-processes --apps slack,zoom,meet`
   → note the bundles with `entrée = oui` or `sortie = oui` (input / output = yes) and the
   recognized app (« app » column). A process that plays the call audio with « — » in the app
   column is a missing prefix (or an XPC process outside the process tree, e.g.
   `com.apple.WebKit.GPU` for Safari).
2. `.build/release/steno-audio --spike-record 60 ~/Desktop/s1-slack --apps slack` (then `zoom`,
   `meet`): the output shows, per app, the processes found, whether the microphone is in use and
   the **title** read through accessibility; `them.wav` must contain the participants and nothing else.
3. Record in `docs/spikes.md` the real window titles (Slack huddle expanded or collapsed, Zoom in
   French, Meet tab with and without a calendar event) to tune `CallTitleHeuristics`.
4. Protocol mode, app not launched yet: send `watch` (Slack) then `start` with
   `"apps":["slack"]` while Slack is closed → recoverable `error tap_failed`; launch Slack and a
   huddle → the log shows « processus de Slack apparus » ("Slack processes appeared") and `them`
   receives the audio.
5. Two simultaneous calls (e.g. Teams and a Slack huddle): two distinct `call` events, and the
   end of one does not affect the other.

### S2 — Transcription

Goal: readable French, technical names understood, under one second per sentence.

1. `.build/release/steno-audio --prefetch-models` (first model download).
2. `.build/release/steno-audio --spike-transcribe ~/Desktop/s1/them.wav`
   then the same on `me.wav`.
3. Read the output: each sentence with its bounds and transcription time; the final summary
   gives the average time, the maximum and the real-time factor. Criterion: maximum < 1 s.
4. Note the mistranscribed technical names: they will go into the vocabulary (after-the-fact correction).
5. Optional: compare with WhisperKit (`whisperkit-cli transcribe --audio-path …`) on the same
   files and record the comparison in `docs/spikes.md`.
6. Check the FluidAudio points marked « À VÉRIFIER (spike S2) » (to verify) in `FluidAudioBackend.swift`.

### S4 — Workstation (company-managed Mac)

Goal: find out right away whether MDM blocks a permission.

1. List the configuration profiles: `sudo profiles show -type configuration` and look for a
   `com.apple.TCC.configuration-profile-policy` payload (`Microphone`,
   `ScreenCapture`/`AudioCapture`, `Accessibility` rules).
2. Réglages Système › Confidentialité et sécurité (System Settings › Privacy & Security): check
   that Micro, « Enregistrement audio du système » (System Audio Recording) (or « Enregistrement
   de l'écran et audio du système », Screen & System Audio Recording) and Accessibilité are
   editable (not greyed out).
3. Run `printf '{"cmd":"permissions"}\n' | .build/release/steno-audio`: you should see `ready`
   then two `permissions` messages (microphone `granted`, accessibility `granted` once authorized).
4. Redo S1 step 4. If tap creation fails, the output says « accès à l'audio système
   refusé » ("system audio access denied") (protocol: `system_audio_denied`). **If a permission is
   blocked: stop and notify the user** (PLAN §13).

### Trying the protocol by hand

```sh
( echo '{"cmd":"watch","apps":[{"id":"teams","name":"Microsoft Teams","bundles":["com.microsoft.teams2","com.microsoft.teams"]},{"id":"slack","name":"Slack","bundles":["com.tinyspeck.slackmacgap"]}]}'
  echo '{"cmd":"start","vocabulary":["Sténo","ms-booking"],"silence_ms":1000,"keep_audio":true,"audio_dir":"/tmp/steno-essai"}'
  sleep 30
  echo '{"cmd":"aside","on":true}'; sleep 5; echo '{"cmd":"aside","on":false}'
  sleep 5
  echo '{"cmd":"stop"}'
  sleep 3
  echo '{"cmd":"diarize","meeting_id":"essai","audio_dir":"/tmp/steno-essai"}'
  sleep 60
) | .build/release/steno-audio
```

Expected: `ready`, `permissions`, `devices`, `call` at each start or end of a Teams or Slack call,
`state listening`, `level` messages (at most 4 per second),
`segment` messages (those spoken during the aside with `"private":true`), `state stopped`, then `diarization`.
End of stdin stops the sidecar.

## Code layout

| File | Role |
|---|---|
| `Sources/StenoAudio/main.swift` | entry point (calls `StenoAudioMain.run`) |
| `StenoAudioMain.swift` | arguments, stdin reader thread, signals, command loop |
| `Protocol.swift` | messages in both directions (Codable), watched apps (`WatchedApp`), device selection (`DeviceSelection`), `bad_command` errors, ISO 8601 timestamps |
| `Output.swift` | serialized stdout output (one line per message), stderr logs, utilities |
| `Controller.swift` | command handling, capture lifecycle |
| `CaptureSession.swift` | microphone + tap + two channels, tap recreation (processes appeared or changed), aside, levels |
| `ChannelPipeline.swift` | per channel: stream gaps, WAV, VAD, segmenter, ASR, vocabulary |
| `CallMonitor.swift` | watched apps (every 2 s): processes per app, debounced call state per app, window reading (accessibility) |
| `CallTitleHeuristics.swift` | pure call-title heuristics (Teams, Slack, Zoom, Meet, default) |
| `ProcessTap.swift` | Core Audio process tap + private aggregate (around the default output) + IOProc |
| `RealtimeAudio.swift` | real-time path: IOProc → lock-free ring → processing queue (`RealtimeSampleBridge`), sample rate tracking, Objective-C exception catching |
| `Sources/StenoAudioRT/` | C / Objective-C: lock-free sample ring written from the IO thread, `@try/@catch` wrapper for AVAudioEngine |
| `MicCapture.swift` | `AVAudioEngine` microphone with AEC, microphone selection, fallback and resumption, restart on device change |
| `AudioDevices.swift` | devices: HAL reading, transport classification, filtering, sorting (pure functions), debounced listeners, setting the device of an AVAudioEngine node |
| `FeedbackPlayer.swift` | `play` sound cue: (pure) synthesis and separate playback engine |
| `Resampler.swift` | mono mixdown, `AVAudioConverter` to 16 kHz |
| `Segmenter.swift` | VAD protocol, energy VAD, utterance segmenter |
| `Transcriber.swift` | `SpeechRecognizer` / `SpeechBackend` protocols, model loading |
| `VocabularyCorrector.swift` | dictionary-based correction (case, accents, separators) |
| `FluidAudioBackend.swift` | **only file depending on FluidAudio** (Parakeet, Silero, diarization) |
| `Diarizer.swift` | formatting of speaker turns (`S1`, `S2`…) |
| `WavWriter.swift` | 16-bit 16 kHz mono WAV (only if `keep_audio`), files with mode 0600 |
| `Permissions.swift` | microphone / system audio / accessibility state |
| `Levels.swift` | RMS levels in dBFS, emitted at most 4 times per second |
| `CoreAudioUtils.swift` | HAL property access, property listeners |
| `Spikes.swift` | `--spike-record`, `--spike-transcribe` modes, diagnostics (`--list-audio-processes`, `--list-devices`), prefetching |

Notable choices:

- A segment's `ts` = local time (with time zone) of the **end** of the utterance; `t0`/`t1` = seconds
  since `start`, on a monotonic clock. When the tap disappears (Teams restarts, sleep), the
  gap is detected and filled (silence in the WAV, clock advanced): `t0`/`t1` and `them.wav`
  stay aligned with `start`, which lets the core match diarization against segments.
- Gaps are filled completely, however long (an app closed for an hour): the WAV advances by as
  much as the segmenter clock, so `retranscribe` and diarization cut at the right place. The WAV
  header is rewritten every 5 s of audio: a sidecar killed before `close` still leaves a readable
  file. The core waits 7 s for the sidecar to exit after `shutdown` (it gives itself 5 s to flush),
  and a broken stdout triggers the same clean shutdown.
- Real-time path: `AudioDeviceCreateIOProcIDWithBlock` calls its block synchronously from the HAL
  IO thread, even with a dispatch queue. The IOProcs (tap and direct microphone capture) therefore
  only mix down to mono into a pre-allocated lock-free ring; a timer drains it every 20 ms on a
  regular queue (rate check, resampling, logs, pipeline).
- The tap aggregate is built around the default output (`kAudioHardwarePropertyDefaultOutputDevice`,
  the one the call plays on), not the sound-effects output, which often stays on the Mac speakers
  while the call is on AirPods. Only the tap's buffers are mixed (the last ones in the aggregate's
  input; a sub-device with a microphone comes first).
- AVAudioEngine raises Objective-C exceptions (AirPods switching between A2DP and HFP while the
  graph is built): every engine call goes through `ObjCException.catching`, so they fall back to the
  direct capture instead of aborting the sidecar. `set_input` gives voice processing a new chance;
  the wake word test outside a meeting records without voice processing (no switch of Bluetooth
  headsets to the hands-free profile).
- `start` asks TCC whether system audio access is denied and emits `system_audio_denied`: in that
  case macOS creates the tap without error and only delivers silence.
- A `me` utterance is `private` if the aside was active at any point during the utterance.
- Call end is declared, app by app, after two consecutive negative readings (≈ 4 s); the 60 s
  grace period remains the core's responsibility.
- The call title is frozen as soon as a plausible title is found during the call; a generic or
  dubious title yields `null` rather than a wrong title (e.g. Slack without a huddle window).
- `watch`: entries without `id` or without a bundle prefix are ignored, duplicate `id`s are ignored
  (the first wins), `name` absent = `id`. A field of the wrong type yields `bad_command`.
- The built-in catalog (`teams`, `slack`, `zoom`, `meet`) is only used by the command-line modes;
  in protocol mode, only the core's `watch` list counts.
- `diarize` error codes: `bad_command` (them.wav missing), `model_missing` (diarization models
  unavailable), `asr_failed` (processing failure).
- `stop` waits for in-progress utterances to be transcribed (at most 15 s) before emitting
  `state stopped`.
- Devices: `start.input_device` absent ≠ `null` (absent = last remembered choice). The microphone
  choice persists from one capture to the next. `device_missing` is emitted only once per absence
  of the microphone (no 30 s rate limiter, unlike other capture errors). Transport classification
  copies the `kAudioDeviceTransportType…` codes so that it stays a testable pure function.

## Points À VÉRIFIER (to verify) on the Mac

The code was written without a Swift toolchain or macOS SDK: it must be built and tested on the Mac.
Uncertain points, in order of priority:

1. **TCC inheritance** by the sidecar launched from Steno.app (S1, step 6); otherwise Rust plan B.
2. **FluidAudio API** (taken from v0.18.4, details at the top of `FluidAudioBackend.swift`):
   `AsrModels.downloadAndLoad(to:version:)`, `AsrManager.transcribe(_:decoderState:language:)`
   (FluidAudio's README still shows `transcribe(samples)`), `language: .french`,
   `VadManager.processStreamingChunk`, `OfflineDiarizerManager.prepareModels()/process(_:)`.
3. **App audio processes** (S1, S1-bis): actual bundles of Teams (`com.microsoft.teams2*`?),
   Slack (`com.tinyspeck.slackmacgap*`), Zoom (`us.zoom.xos`, `us.zoom.CptHost`? — prefixes of the
   built-in catalog) and browsers; whether a rendering process exists outside the prefixes and
   outside the process tree (e.g. XPC `com.apple.WebKit.GPU` for Safari, not covered: Meet in Safari
   will probably be neither detected nor captured) → `--list-audio-processes --apps …`.
4. **`CATapDescription`**: `stereoMixdownOfProcesses:` init with `[AudioObjectID]`, properties
   `name`, `isPrivate`, `muteBehavior`, `stereoGlobalTapButExcludeProcesses:` init (plan B).
5. **System audio denial**: OSStatus code returned (assumed `'nope'` or `'!hog'`), or tap
   created normally but silent (then the state stays `unknown`; `granted` as soon as non-zero
   audio arrives).
6. **Voice processing**: channel 0 = processed voice; the
   `voiceProcessingOtherAudioDuckingConfiguration` setting (Teams audio must not be lowered); whether
   `mainMixerNode` is needed; fallback without AEC if startup fails.
7. **Reconfiguration loop**: check that creating the tap's private aggregate does not trigger
   `AVAudioEngineConfigurationChange` on the microphone engine (restart loop).
8. **Real window titles** (`CallTitleHeuristics`): new Teams in French; Slack huddle window
   (exact title, does it exist when the huddle is collapsed?); Zoom in French
   (« Réunion Zoom »?); Meet tab (« Meet – <événement> », exact separator); `AXMain`
   and `AXFocusedWindow` attributes (compared with `CFEqual`).
9. `proc_pidinfo` / `PROC_PIDTBSDINFO` reachable through `import Darwin` (process ancestry).
10. Receiving `NSWorkspace.didWakeNotification` in a binary without `NSApplication`
    (`RunLoop.main.run()` loop).
11. Linker option `-sectcreate __TEXT __info_plist` (`unsafeFlags`) accepted by
    `swift build` for the root package.
12. Actual end-of-sentence → segment latency (< 3 s targeted) with `silence_ms = 1000` and 256 ms
    VAD frames.
13. **`NSWorkspace.shared.runningApplications`** read on the monitor queue (off the main thread)
    in a binary without `NSApplication`: is the list up to date? (otherwise go back to
    `NSRunningApplication.runningApplications(withBundleIdentifier:)` per known prefix). Filter
    `activationPolicy != .prohibited` to avoid querying accessibility on helper processes.
14. **Multi-app tap**: `CATapDescription(stereoMixdownOfProcesses:)` with the processes of
    several apps (union); recreation without an audible click when a helper process appears.
15. **Detection false positives**: a watched browser using the microphone outside Meet
    triggers a `call`; Slack may open the microphone outside a huddle (microphone test in the
    preferences).
16. **Chosen microphone and voice processing** (S1-ter): is `kAudioOutputUnitProperty_CurrentDevice` on
    the `inputNode`'s `AudioUnit`, set before `setVoiceProcessingEnabled(true)`, kept, or is the
    second setting (after) needed? With VoiceProcessingIO, does the property really target the
    input (and not the output)? Does voice processing accept a microphone other than the default
    one (otherwise: fallback without AEC, visible in the log)?
17. **Sound cue output**: `kAudioOutputUnitProperty_CurrentDevice` on the `outputNode` of an
    engine with no input, set before the mixer is connected; mono centered on both channels;
    cue not attenuated by the microphone voice processing's "ducking".
18. **Private aggregates visible** in our process's `kAudioHardwarePropertyDevices`: actual
    prefixes (`steno-tap-`, `CADefaultDeviceAggregate`, `VPAUAggregateAudioDevice`); failing that, read
    the `private` key of `kAudioAggregateDevicePropertyComposition`.
19. **AirPods**: input channels visible as soon as they appear in the list (otherwise the microphone is
    resumed on the next list or configuration change, not immediately); A2DP → HFP switch delay
    when moving to the AirPods microphone (longer gap than with a wired microphone).
20. **Removing a HAL listener from its own queue** (`AudioObjectRemovePropertyListenerBlock`
    called on the callback queue, in `MicCapture.stop()` and `AudioDeviceWatcher.stop()`): no
    deadlock.

## To test first on a Mac

1. `swift build -c release` then `swift test` (fix any compilation errors).
2. Full spike S1, especially step 6 (launch from an empty Tauri app), then S1-bis (Slack,
   Zoom, Meet).
3. Spike S2 on the S1 recordings (`--spike-transcribe`).
4. Spike S4 on the work Mac.
5. Manual protocol test (above), then unplug the headset during capture.
6. Spike S1-ter (devices): `--list-devices`, `set_input` while listening, AirPods put away
   then taken out, `play` on the AirPods and on the default output.
