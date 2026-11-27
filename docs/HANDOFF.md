# Handover — Sténo project status and remaining work

> Document for the person (or agent) taking over the project. Read it alongside `PLAN.md`
> (the requirements), `docs/contracts.md` (the interfaces), `docs/adapters.md` (the agent CLIs),
> `docs/spikes.md` (what must be validated on a Mac).

## 1. Summary

All the software described in `PLAN.md` is **written**. Everything that does not depend on macOS
has been **run and tested on Linux**: Rust core, CLI, UI, and the Tauri app launched on a virtual
display, with a simulated audio sidecar and a fake agent CLI.

Three things have **never run**:
- the Swift audio sidecar (`audio/StenoAudio`), for lack of a Mac;
- the real agent CLIs with a real subscription;
- Teams capture.

That is the purpose of phase 0 (`docs/spikes.md`), to be done first on a Mac.

Product rules added after the initial plan (applied everywhere):
0. **Multi-app**: call detection covers a configurable list of apps (`audio.call_apps`:
   Teams, Slack, Zoom, Webex, Discord, FaceTime; a browser for Google Meet, disabled by
   default). The core sends the list to the sidecar (`watch`), the sidecar emits `call` per app, and
   `start.apps` names the app whose audio is captured. The meeting keeps `call_app`.
1. **Listening never starts automatically.**
   - A detected Teams call produces at most a notification « Appel Teams détecté — démarrer l'écoute ? » ("Teams call detected — start listening?").
   - Listening is started from the macOS status bar icon (native menu), the ⌃⌥S shortcut, or the app.
2. **Listening never stops automatically.**
   - The end of the Teams call does not stop it.
   - It is stopped from the menu, the shortcut, the app, or **by voice**: « Sténo, arrête l'écoute » ("Sténo, stop listening"; intent `stop`).
3. Design: clean, navy-blue sidebar, the user's logo (`assets/logo.jpg`, recreated as SVG in the UI, icons generated in `app/src-tauri/icons/`).

## 2. Repository layout

| Folder | Contents | Size |
|---|---|---|
| `crates/steno-core` | Rust lib: SQLite database, settings, job queue, live session, wake word, echo suppression, agent adapters, read-only policy, report, correlation, diagnostics, API dispatcher | ~8,700 lines |
| `crates/steno-cli` | `steno` bin: ask, doctor, init, export, mcp, meetings, settings, jobs, replay, simulate, serve, daemon, policy-hook | ~1,000 lines + 580 of tests |
| `app/src-tauri` | Tauri 2 app: status bar, windows, global shortcuts, notifications, the 49 commands | ~800 lines |
| `app/ui` | Svelte 5 + Vite + TS, fake backend, HTTP backend, Vitest and Playwright tests | ~7,500 lines |
| `audio/StenoAudio` | Swift sidecar: Teams process tap, microphone with echo cancellation, FluidAudio, diarization, spikes | ~3,700 lines + 500 of tests |
| `crates/steno-audio-linux` | Rust sidecar for Linux, same protocol: PulseAudio / PipeWire, WebRTC AEC, sherpa-onnx (Parakeet, Canary, Silero, pyannote), EDS / .ics calendar | ~5,000 lines, tests included |
| `agent/` | `INSTRUCTIONS.md`, playbooks (`playbooks/`), `policy.toml` | |
| `fixtures/` | synthetic 45-min daily, expected results, fake `claude` | |
| `docs/` | contracts, adapters, spikes, screenshots, this document | |

## 3. Architecture at a glance

```
Steno.app (Tauri) ──┬─ UI Svelte (WKWebView) ── invoke / events ──────┐
                     └─ steno-core (Rust, same process) ◄──────────────┘
                          │  JSON lines (stdin/stdout)          │ runs claude/codex… (-p, read-only)
                          ▼                                     ▼
                     steno-audio (Swift)                  agent CLI + "policy-hook" hook
                     Teams tap + mic → VAD → ASR            (the Sténo binary itself)
CLI `steno`: same core; `steno serve` = same API over HTTP + SSE (e2e tests, browser dev)
```

Key points:
- **A single source of truth for the API**: `docs/contracts.md` §5. The Tauri app
  (`app/src-tauri/src/commands.rs`) and `steno serve` (`steno_core::api::dispatch`) expose
  exactly the same 49 commands. Two tests check this:
  - `api::tests::toutes_les_commandes_du_contrat_sont_connues`;
  - `tests::commandes_tauri_conformes_au_contrat`.
- **Agent security** (`crates/steno-core/src/policy.rs`, `agents/claude_code.rs`). With Claude Code:
  - `dontAsk` mode and a tool allowlist;
  - explicit denials of write tools;
  - a `PreToolUse` hook → `<Sténo executable> policy-hook <policy.json>`, which denies write MCP tools (patterns and keywords), shell commands not on the list, reads outside the checked repositories, and enforces the call budget.

  Agents without an MCP deny list (Codex, Gemini, OpenCode, custom) have their connectors greyed out and disabled.
- **The core is the only writer** to the database. The CLI writes only to `jobs`, then runs the job itself if the app is not running (heartbeat `runtime.app_heartbeat`).
- **Meeting lifecycle**:
  1. `start_meeting` (manual) → `Start` to the sidecar → segments → echo suppression → wake word (`me` channel only) → requests;
  2. `stop_meeting` (manual or « arrête l'écoute », "stop listening") → `diarize` job (if audio) then `report`;
  3. then `correlate` if the mode is `auto` and the meeting is not excluded;
  4. audio is deleted according to the retention policy.

## 4. What is done and verified

**Linux port (Debian / Ubuntu)**: sidecar `crates/steno-audio-linux` (see its README for
the tests performed: simulated call on PulseAudio, echo cancellation, microphone unplugged,
sound server restart, full chain with `steno serve`), core and app adapted
(per-platform `call_apps`, document extraction without `textutil`, diagnostics,
`steno-app --toggle/--aside/--note/--show` actions for shortcuts under Wayland, full status bar
menu), `make deps`, `make install` (into `~/.local`), `make deb`. Still to validate on
a real desktop: real Teams for Linux / Slack / Zoom clients, native PipeWire, GNOME
AppIndicator icon, clicking on notifications.


| Area | Status | Evidence |
|---|---|---|
| SQLite schema, FTS5, migrations, backup, retention | ✅ | tests `db::*`, `maintenance` |
| Typed settings, team TOML import/export | ✅ | tests `settings::*`, `cli::reglages_d_equipe_sans_donnees_perso` |
| Wake word: detection, strict mode, variants, forbidden words, checks (syllables, history, first names), aside, voice stop | ✅ | tests `wake::*`, acceptance §6.5 (≥ 18/20, 0 false triggers) |
| Echo suppression | ✅ | tests `echo::*`, daily replay |
| `claude-code` adapter | ✅ on fake CLI | acceptance §5.6 (sources, session resume, refusal) |
| `codex`, `gemini`, `opencode`, `custom` adapters | 🟡 written, output parsing tested, never actually run | parser unit tests |
| Read-only policy + hook | ✅ | tests `policy::*`, `cli::crochet_de_politique`, `doctor --live` on fake CLI |
| Report, decisions, action items, suggested speakers, drafts | ✅ on fake CLI | acceptance §7.3 |
| Correlation (off / on demand / auto, exclusions, versions) | ✅ on fake CLI | acceptance §7.3 + `exclusions_et_confidentialite` |
| Manual listening only; end of Teams call has no effect; voice stop | ✅ | `appel_teams_propose_sans_demarrer_et_arret_a_la_voix`, `daily_complete…` |
| CLI (ask, export, mcp, settings, policy-hook, serve, simulate) | ✅ | `crates/steno-cli/tests/cli.rs` (binary as a subprocess) |
| Read-only MCP server | ✅ | `cli::serveur_mcp_lecture_seule` |
| Search performance (500 meetings, 150,000 sentences) | ✅ 16 to 75 ms | `cargo test -p steno-core --release --test perf -- --ignored` |
| Tauri app (commands, events, status bar, shortcuts) | 🟡 compiled and launched on Linux; native macOS menu never seen | command conformance test; manual run under Xvfb |
| UI (every screen in the plan) | ✅ | Vitest + Playwright (mock and real), screenshots `docs/screenshots/` |
| Swift sidecar | 🔴 never compiled | — |

## 5. Tests

Status at handover: **everything is green**.
- Rust: 74 tests in total:
  - 58 core unit tests;
  - 3 app tests;
  - 1 CLI unit test;
  - 7 CLI functional tests;
  - 5 acceptance tests.
- Vitest: 66 tests.
- Playwright: 16 tests, 14 on the mock and 2 real scenarios.
- Performance: test green.

| Command | Contents |
|---|---|
| `cargo test --workspace` | Rust: unit (core, app), functional (CLI as a subprocess), acceptance (full pipeline with the fake agent), command conformance |
| `cargo test -p steno-core --release --test perf -- --ignored` | search performance |
| `cd app/ui && npm run test:unit` | Vitest: formatting, markdown sanitizing, router, pure logic |
| `cd app/ui && npm run test:e2e` | Playwright: `maquette` project (UI + fake backend) and `reel` project (UI → `steno serve` → core → fake `claude` → SQLite) |
| `make test` | all of the above |
| `swift test` (in `audio/StenoAudio`) | protocol, vocabulary correction, segmenter, utilities (**never run**) |

Test tools:
- `fixtures/daily-45min.jsonl` (regenerable with `fixtures/generate.py`) with `fixtures/expected.json`;
- `fixtures/fake-bin/claude`, which mimics `claude -p --output-format stream-json`, `--version` and `mcp list`, and **actually** calls the hook declared in `--settings`;
- `steno replay <fichier> --speed N`, the sidecar simulator: it announces the Teams call then waits for `start`.

On a Mac, for the e2e tests: `cd app/ui && npx playwright install chromium` once.

## 6. Remaining work (by priority)

### P0 — Phase 0 on a Mac (blocking, details in `docs/spikes.md`)
1. **Compile the sidecar** (`cd audio/StenoAudio && swift build -c release && swift test`) and fix the errors. Uncertain points are marked `À VÉRIFIER` (to verify) in the code and listed in `audio/StenoAudio/README.md`:
   - FluidAudio 0.18.x API: `AsrManager.transcribe(_:decoderState:language:)`, `VadManager`, `OfflineDiarizerManager`;
   - `CATapDescription` initializers;
   - error code when system audio is denied.
2. **S4** (work Mac, MDM): if system audio is blocked, stop and report it.
3. **S1**: Teams capture, then **Slack (huddle) and Zoom**, **launched from Steno.app**, to check that the permission covers the sidecar. Plan B: capture in Rust in the main process (`objc2-core-audio`), with the same protocol.
4. **S2**: French transcription quality and latency (< 1 s per sentence).
5. **S3**: `steno doctor --live` with the real Claude Code then Codex:
   - are the account's connectors available in `-p`?
   - are the Codex options (`-c mcp_servers={}`, `exec resume`) valid?
6. **S5**: idle RAM < 120 MB. Models are loaded on `start` and released on `stop`; `STENO_PRELOAD_MODELS=1` preloads them.

### P1 — App checks on macOS
- `make install`, then the first-run wizard end to end.
- **Status bar**: clicking opens the native menu, labels follow the state (« ● Écoute en cours — … » "Listening —", « Appel Teams détecté : … » "Teams call detected:"), the « ● » appears next to the icon, the « Réunion confidentielle » ("Confidential meeting") checkbox works.
- Global shortcuts: ⌃⌥S, ⌃⌥Space held (aside, with press and release events), ⌃⌥N.
- macOS notifications. Clicking a notification does not open the answer yet: `tauri-plugin-notification` does not handle the click action on desktop, and the `route` field is ready in `CoreEvent::Notify`.
- Dock: the icon appears when the window is open and disappears when it closes (`ActivationPolicy`).
- Signing: `make install` signs with the local identity "Steno Local Signing" (scripts/local-signing.sh); the signing requirement (identifier + certificate) is stable, so TCC permissions survive from one build to the next. When signed ad hoc, the app lost them on every build.
- Audio feedback when a command is detected (`afplay`): must never be injected into the Teams audio.

### P2 — Features to refine
- Wake word: tune the verbs and variants on real Parakeet transcripts, and run through the voice test on the Voice screen on a Mac.
- Diarization: matching speaker turns to `them` segments, suggested renaming (« Marie ? ») to validate in the UI.
- Prompts (`crates/steno-core/src/prompts.rs`, `agent/playbooks/`): to adjust on real answers (`TL;DR` format, final JSON block of the report and of the correlation).
- `codex`, `gemini`, `opencode`: check the options on the installed versions, then update `docs/adapters.md`. If a CLI can block MCP tools by pattern, set `tool_deny_list: true` and translate the policy.
- Pause / resume listening: the `paused` state exists in the protocol but is not exposed.
- Clicking a notification → open the answer; "last 5 answers" directly in the native menu.

### Notes left by the UI work (non-blocking)
- `get_settings` still returns `audio.end_grace_s` (obsolete, ignored): to remove in a settings migration.
- `steno://navigate` is not emitted by `steno serve` (only by the Tauri app). The UI subscribes to it without depending on it.
- In simulation, there is only one speaker « Eux » ("Them"): no diarization without audio. The duration is 0 min because of the accelerated replay.
- In HTTP mode, `openMain(route)` changes the route within the page and `openExternal` opens a tab.

### Agents, models, devices: to verify on a Mac
- **Antigravity** (`agy`) and **Grok Build** (`grok`):
  - options written from the official docs, never run (see `docs/adapters.md`);
  - to verify: `agy -p … --output-format json`, the actual shape of `grok`'s JSON output, the `--disallowed-tools` syntax, and `--effort` depending on the model.
- **Claude Code**: verify `--effort` on the installed version.
- **Codex**: verify that `-c model_reasoning_effort=…` is accepted by `codex exec`.
- **Devices**:
  - microphone selected with echo cancellation (`kAudioOutputUnitProperty_CurrentDevice` on `inputNode`);
  - switching microphone while listening;
  - AirPods connected then disconnected (`device_missing` fallback);
  - signal played on the selected output;
  - `steno-audio --list-devices`.
  - Full list in `audio/StenoAudio/README.md`, items 16 to 20.

### Design v2: to verify on a Mac
- Native translucency: the sidebar at ~60% opacity over the "sidebar" material, legibility depending on the wallpaper, active or inactive window, and the transparent `#/tray` panel over the "popover" material (`tauri.conf.json`: `transparent`, `windowEffects`, `macOSPrivateApi`).
- Arc-style sidebar:
  - position of the floating button next to the window buttons (`--tb-left: 78px`, `--tb-top: 2px` in `Shell.svelte`);
  - window drag area in collapsed mode;
  - ⌘\ on an AZERTY keyboard.
- View Transitions in macOS 14.2+ WebKit.
- Playwright test of the ⌘K palette (`maquette.spec.ts`, ↑↓ navigation): an isolated failure was seen once, then it passed more than 30 times. If the failure comes back, look for what closes the palette right after a navigation (focus loss, view transition).

### P3 — Team and polish
- Team configuration: write a starter TOML (service vocabulary, verbs, exclusions).
- Make a real Mac available in CI (`macos` job on `workflow_dispatch` only, since macOS minutes cost more).
- GDPR consent: information notice for participants (outside the code).

## 7. Useful commands

```bash
make test                     # all tests
make serve                    # UI + real core in the browser: http://127.0.0.1:4180/?backend=http
make simulate                 # sample daily through the whole pipeline, text output
make dev                      # Tauri app in development (simulated sidecar, fake agent)
make install                  # on a Mac: build, sign, install Steno.app + CLI + models
steno doctor [--live]        # diagnostic
STENO_HOME=/tmp/x steno …   # isolated data folder
RUST_LOG=debug                # verbose logs (app: ~/Library/Application Support/Steno/logs/)
```

## 8. Known pitfalls

- `tauri-build` requires `app/src-tauri/binaries/steno-audio-<triplet>`. On Linux,
  `make sidecar-placeholder` creates a placeholder; on a Mac, `make sidecar` copies the Swift binary.
- In debug, the app loads the UI from Vite (`http://localhost:5173`): run `npm run dev` in
  `app/ui`, or use `make dev`.
- UI dates tolerate non-ISO values: due dates come from the AI (« vendredi », "Friday").
- The policy hook is the current executable (`current_exe()`): if the app is moved
  during an agent run, the request fails cleanly (tool denied).
- A child process started by a test must have stdout/stderr set to `null`, otherwise the
  `cargo test` pipe stays open after a failure.
