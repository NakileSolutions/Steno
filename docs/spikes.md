# Phase 0 — spikes

> **Status**: the code was written in a **Linux** environment (no Mac, no Teams, no agent
> subscription). Everything that does not depend on macOS was verified here (see "Done" further
> down). Spikes S1 to S5 **still have to be run on a Mac**: they are blocking before any real
> use. The detailed sidecar procedures are in `audio/StenoAudio/README.md`.

## Summary

| Spike | Status | What was done here | What remains to do on the Mac |
|---|---|---|---|
| S1 Capture | ⏳ to do | Swift sidecar written (process tap on the whole Teams tree, AVAudioEngine + voice processing, `--spike-record`); **not compiled** (no Swift toolchain here) | Compile, record `me.wav` + `them.wav` during a real call, **launched from Steno.app** |
| S2 ASR | ⏳ to do | FluidAudio integration (Parakeet TDT v3, Silero VAD) behind a protocol; `--spike-transcribe` | Measure French quality and latency (< 1 s per sentence); compare WhisperKit if needed |
| S3 Agents | 🟡 partly | `claude` 2.1.332 options checked (`--help`); read-only policy tested end to end with a fake `claude` that actually calls the hook | `steno doctor --live` with the real Claude Code then Codex: MCP visible in `-p`, writes denied |
| S4 Workstation | ⏳ to do | `steno doctor` shows the permission state reported by the sidecar and the steps to follow | Grant microphone, system audio, accessibility on the work Mac; if MDM blocks it: **stop and report** |
| S5 Footprint | ⏳ to do | Tauri app compiled and launched on Linux (Xvfb) with the real core | Measure Steno.app idle RAM (target < 120 MB) |

## S1 — Capture (blocking)

Goal: two audible tracks (`me.wav`, `them.wav`) during a real Teams call, **with the sidecar
launched by the app** (the « Enregistrement audio du système » ("System Audio Recording")
permission granted to Steno.app must cover the embedded sidecar).

```bash
cd audio/StenoAudio && swift build -c release
.build/release/steno-audio --list-audio-processes        # find Teams' audio processes
.build/release/steno-audio --spike-record 60 ~/Desktop/s1 # directly from the Terminal
make install && open ~/Applications/Steno.app             # then start a meeting from the app
```

Criteria: audible, separate tracks; unplug / replug the headset → recovery < 5 s.
If the permission does not cover the sidecar: plan B = capture in Rust (`objc2-core-audio`) in
the main process (the JSON lines protocol stays the same, the transport becomes internal).

## S2 — Transcription

```bash
.build/release/steno-audio --prefetch-models
.build/release/steno-audio --spike-transcribe ~/Desktop/s1/them.wav
```

Criteria: readable French, technical names (ms-booking, APP-1234) understood or corrected by the
vocabulary, < 1 s per sentence on Apple Silicon. Points to check in the code:
`audio/StenoAudio/README.md` § « Points À VÉRIFIER » (notably the FluidAudio 0.18.x API).

## S3 — Agents

```bash
steno doctor --live
```

1. Claude Code: `claude mcp list`, then check in Sténo (Réglages → Connecteurs →
   Rafraîchir, i.e. Settings → Connectors → Refresh) that the account's connectors appear and that a live request can
   use them in `-p` mode. Plan B if missing: `claude mcp add --transport http …`.
2. Try a write tool: `doctor --live` asks the agent to post on Slack and to
   create a ticket; no write tool may appear in `tools_used`.
3. Codex: check `codex exec --json`, `codex exec resume`, `-c mcp_servers={}`
   (see `docs/adapters.md`).

Verified here (`cargo test -p steno-cli --test acceptance`) with `fixtures/fake-bin/claude`, which
mimics `claude -p --output-format stream-json` and calls the hook declared in `--settings`:
- the `mcp__slack__slack_send_message` call is denied by the hook even if the Slack
  connector is checked; `git push` is denied; `rg` goes through;
- a follow-up in the same meeting resumes the session (`--resume <id>`);
- « Poste ça sur Slack » ("Post this on Slack") is refused locally, without calling the agent.

## S4 — Workstation

On the work Mac: launch Steno.app, accept the three prompts, then `steno doctor`.
If « Audio du système » ("System audio") is denied or missing from System Settings (MDM profile):
**stop the project on this machine and report it** (no workaround).

## S5 — Footprint

```bash
make install && open ~/Applications/Steno.app
# window closed, no meeting, after 1 min:
ps -o rss=,comm= -p $(pgrep -f Steno.app/Contents/MacOS) | awk '{s+=$1} END {print s/1024 " MB"}'
```

Target: < 120 MB in total (app + WebKit + sidecar), CPU < 1%. At idle the sidecar only
watches Teams (one check every 2 s); models are only loaded on `start`.

## Done in this environment (Linux)

- `cargo test --workspace --exclude steno-app`: 60+ unit tests + 4 acceptance tests
  (§5.6 sourced requests / session / refusals, §6.5 wake word 18+/20 and 0 false triggers,
  §7.3 report and correlation, exclusions, confidentiality).
- Performance: full-text search over 500 meetings / 150,000 sentences: ~16 ms; filtered
  list: ~75 ms (`cargo test -p steno-core --release --test perf -- --ignored`).
- `steno simulate fixtures/daily-45min.jsonl`: the 45-min daily goes through the whole
  pipeline (simulated sidecar → meeting → 2 voice commands → report).
- Tauri app launched under Xvfb with the real core, the simulated sidecar and the fake agent:
  wizard, live meeting, requests, report, correlation checked on screen.
- UI: `npm run check` with no errors, bundle ~104 KB gzip (target < 300 KB).
