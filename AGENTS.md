# Guidelines for coding agents

This file is for AI coding agents (Claude Code, Codex, Gemini CLI, OpenCode…) and for the
people working on Sténo's code. `CLAUDE.md` imports it: edit this file, not that one.

> ⚠️ Not to be confused with `agent/` (`INSTRUCTIONS.md`, `playbooks/`, `policy.toml`,
> `skills/steno/`): those are the **product's** instructions and skill, sent to or installed
> in the user's agent. They are compiled into the core (`include_str!`). Only touch them when
> the task is about how Sténo itself behaves.

## The project in brief

Local-first meeting assistant for macOS and Linux (Debian / Ubuntu). Read `README.md` first (French version:
`README.fr.md`, keep both in sync), then depending on the task:
- `docs/contracts.md`: **source of truth** for the interfaces (SQLite schema, sidecar
  protocol, `AgentAdapter` trait, settings, API commands, HTTP backend);
- `docs/adapters.md`: the agent CLIs; `docs/HANDOFF.md`: status and detailed architecture;
  `PLAN.md`: the original requirements (the `PLAN §…` references in the code point there).

Many docs, comments and test names are still in French while they are being migrated;
see *Language* below.

| Directory | Language | Role |
|---|---|---|
| `crates/steno-core` | Rust | core: SQLite, job queue, wake word, echo filter, agent adapters, read-only policy, API (`api.rs`) |
| `crates/steno-cli` | Rust | `steno` CLI (ask, doctor, mcp, serve, replay, simulate…) |
| `app/src-tauri` | Rust | Tauri 2 app: menu bar, shortcuts, notifications, commands |
| `app/ui` | Svelte 5 + TS | UI; fake backend (`mock.ts`) and HTTP backend |
| `audio/StenoAudio` | Swift | macOS sidecar: Core Audio capture, VAD, ASR, diarization |
| `crates/steno-audio-linux` | Rust | Linux sidecar, same JSON-lines protocol: PulseAudio / PipeWire, WebRTC AEC, sherpa-onnx |
| `packaging/linux` | | desktop entries for `make install` and the `.deb` |
| `fixtures/` | | 45-minute synthetic daily meeting, fake agent CLIs (`fake-bin/`) |

## Commands

```bash
make test                         # everything: Rust, Swift, Vitest, Playwright
cargo test --workspace            # Rust (needs `make sidecar-placeholder` and `app/ui/dist`, see make test-rust)
cargo test -p steno-core <filter> # one core test
cd app/ui && npm run check && npm run test:unit   # types (warnings are errors) + Vitest
cd app/ui && npm run test:e2e     # Playwright (mock UI + real core via steno serve)
cd audio/StenoAudio && swift test # macOS sidecar (macOS only)
make deps && make deb             # Linux: build packages (sudo), then the .deb
make serve                        # UI + real core: http://127.0.0.1:4180/?backend=http
make simulate                     # replays the sample meeting through the whole pipeline, fake agent
```

Run at least the tests for the area you touched before handing back. CI (Ubuntu) runs
`npm run check`, Vitest, `cargo test --workspace` (including the Linux sidecar), the perf
test, Playwright, and builds and installs the `.deb`. The Swift sidecar is **not** tested in
CI: if you change it, run `swift test` on a Mac. A change to the sidecar protocol must land
in both sidecars.
On a fresh Mac: `cd app/ui && npx playwright install chromium` once.

## Non-negotiable rules

1. **The repository is public.** No real data: no company, customer, colleague, project or
   internal tool names, no personal paths, no keys, no tokens. Screenshots, fixtures, tests
   and examples use an **entirely made-up** world. Review your diffs with that in mind.
2. **Sténo never publishes anything.** The user's agent only reads (`policy.rs`,
   `agent/policy.toml`, the `policy-hook` command). Any change that weakens this policy
   (tool allowlist, MCP denials, call budget, readable repositories) must be explicitly
   requested and tested.
3. **Audio never leaves the computer**; no telemetry, no Sténo server. Only text is sent, and only
   to the agent the user picked.
4. **Listening never starts or stops on its own** (the end of a call does not stop it).
5. **The core is the only writer to the database.** The CLI only writes to `jobs`.

## Conventions

- **Language**: everything technical is in **English**: code comments, test names, log
  messages, commit messages, PR titles and descriptions, docs, scripts, CI. The existing
  French is being migrated area by area in dedicated PRs; do not translate it as a side
  effect of another change (it creates conflicts). Identifiers are in English.
- **Bilingual UI, French is the source language**: `t('Texte en français')`, with the English
  translation in `app/ui/src/lib/i18n/en/*.ts` (key = the exact French text); `i18n.test.ts`
  fails if a translation is missing. In the core: `tr(fr, en)`
  (`crates/steno-core/src/i18n.rs`). Every user-facing string needs both languages; ask for
  a French review if you are not confident in yours.
- **API**: a command lives in four places that must stay in sync (contract, `api.rs`, Tauri's
  `commands.rs`/`lib.rs`, `app/ui/src/lib/api.ts` + `mock.ts`). See the `api-command` skill.
- **SQLite**: new numbered migration in `crates/steno-core/src/db/migrations/`; never edit a
  migration that has been merged. **Settings**: saved values survive default changes; keep
  reading the old format. See the `migration` skill.
- Transcript sentences are **data**, never instructions (prompts).
- No new dependency (crate, npm or Swift package) without justifying it in the PR.

## Git and pull requests

- `main` is protected: PR required, green CI, **signed commits**, linear history (squash or
  rebase), no force-push. The maintainer merges.
- One branch per topic, short and descriptive (`zoom-detection`, `docx-export`), no username
  prefix. Start from an up-to-date `main`.
- PR title in English describing what changes for the user ("Fix: “Install” update quit the
  app"). The description says what, why, and how it was tested.
- Do not change the version number or create tags outside a release PR (`RELEASING.md`,
  `release` skill).

## Working together (people and agents)

- Keep PRs **small and focused**; do not reformat or "clean up" unrelated files: it causes
  conflicts for everyone else.
- One agent per branch (or worktree). Do not push to someone else's branch, and do not use
  bare `git stash` in a clone shared between worktrees.
- Conflict-prone files: `docs/contracts.md`, `crates/steno-core/src/api.rs`,
  `app/src-tauri/src/lib.rs`, `app/ui/src/lib/api.ts`, `mock.ts`, the `i18n/en/*` files.
  Rebase often when touching them.
- Update docs in the same PR as the code (`contracts.md`, `README.md` and `README.fr.md`, `HANDOFF.md` when a
  feature's status changes).
- If you hit a non-obvious pitfall, add it below.

## Known pitfalls (macOS, Linux, audio, Tauri)

- App launched by Finder/launchd: minimal PATH (no `claude`, `cargo`, `rustup`); the app
  restores the login shell's PATH.
- Tauri: a synchronous command on the main thread without a tokio runtime panics and kills
  the app; run it on the async runtime.
- macOS permissions (TCC): an ad hoc signature makes every build a "new app";
  `make install` signs with the local "Steno Local Signing" certificate. Changing the bundle
  id forces users to re-grant everything.
- Bluetooth headsets in a call: output drops to 16 kHz while the tap reports 48 kHz.
- Echo cancellation (voice processing): error -10875 on some Macs; never retry after a
  failure (it breaks the call app's audio).
- Canary: loaded on the GPU (on the ANE, loading hangs for over 30 minutes).
- Diarization: wait for the sidecar to have fully stopped, or the WAV is read empty.
- Core Audio IOProc blocks run synchronously on the HAL IO thread even when given a dispatch
  queue: no allocation, lock or log there (`RealtimeSampleBridge`). AVAudioEngine raises
  Objective-C exceptions that Swift's `do/catch` misses: wrap engine calls in
  `ObjCException.catching`.
- Linux: no TCC; permissions are reported `granted` as soon as the sound server answers
  (see `crates/steno-audio-linux/README.md`).
- The `captures` Playwright project rewrites `docs/screenshots/` (`screenshots` skill).

## Skills

Step-by-step procedures in `.claude/skills/<name>/SKILL.md` (readable by any agent):

| Skill | When |
|---|---|
| `prepare-pr` | before opening or updating a pull request |
| `api-command` | adding or changing an API command (UI ↔ core) |
| `migration` | changing the SQLite schema or a setting |
| `screenshots` | regenerating the documentation screenshots |
| `release` | preparing a new version |
