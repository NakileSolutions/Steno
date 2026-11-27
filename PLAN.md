# Sténo — local meeting assistant (Mac + Teams) — plan v3

> Implementation plan intended for coding agents (Orca, Claude Code, Codex…).
> **Goal**: a 100 % local equivalent of Granola, extended with cross-searches across
> codebase / Jira / Confluence / Datadog that can be triggered **by voice** during the meeting,
> with a **lightweight Rust desktop app** that works without Orca.
> **Constraints**:
> - €0 of cost on top of the AI subscription each dev already has (Claude Code, Codex, Gemini CLI, OpenCode…);
> - **configurable** AI agent: nothing in the code depends on a provider;
> - each dev builds the tool themselves (section 14);
> - no Electron: a **Tauri 2 (Rust)** app, with minimal memory and CPU footprint.

---

## 0. How to use this plan

1. Create a local `steno/` repository (private, never pushed to a public repository) and put this file in it.
2. First agent: « Lis PLAN.md, écris `docs/contracts.md` (schéma SQLite section 3 + protocole section 4.3), puis exécute la phase 0 et rends-moi `docs/spikes.md`. » ("Read PLAN.md, write `docs/contracts.md` (SQLite schema from section 3 + protocol from section 4.3), then run phase 0 and give me `docs/spikes.md`.")
3. Once phase 0 is validated: **4 agents in parallel** (section 11).

Rules for all agents:
- French in the UI, notifications and reports.
- **Sténo never publishes anything externally** (Slack, Jira, Confluence, GitLab, Datadog). It only reads; anything that would go out is a draft the user validates themselves.
- No calls to any paid API. Models are only called through the **official CLI of the chosen agent**, within the dev's subscription, never through a homemade client that would reuse a token.
- No provider, model or CLI option names outside the adapters (`crates/steno-core/src/agents/`) and the settings.
- Nothing dev-specific in the repository (team, paths, wake word, models): it all lives in the database.

---

## 1. Overview

```
┌──────────────────────────── Steno.app (Mac) ─────────────────────────────┐
│                                                                            │
│  ┌─ steno-audio (Swift sidecar, binary embedded in the app) ────────────┐  │
│  │  Mic (AVAudioEngine + AEC)             → "me" channel   (Moi)        │  │
│  │  Core Audio process tap on Teams       → "them" channel (Eux)        │  │
│  │  Teams call detection, window title (Accessibility)                  │  │
│  │  Silero VAD + Parakeet TDT v3 ASR (FluidAudio, CoreML / ANE, FR)     │  │
│  │  ⇄ JSON lines over stdin/stdout (section 4.3)                        │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                         │ segments                                         │
│  ┌─ steno-core (Rust, inside the Tauri process) ────────────────────────┐  │
│  │  SQLite (WAL + FTS5): meetings, transcript, requests, reports,       │  │
│  │  decisions, actions, settings                                        │  │
│  │  Wake-word detection ("me" channel only)                             │  │
│  │  Job queue: live requests, report, correlation…                      │  │
│  │  Agent adapters → launch claude / codex / gemini / opencode…         │  │
│  │  Shared read-only policy                                             │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
│  ┌─ UI (Tauri 2: system WKWebView + Svelte) ────────────────────────────┐  │
│  │  Menu bar icon · Meetings / Search / Actions / Settings window ·     │  │
│  │  "live" view                                                         │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────┘
   steno (Rust CLI): ask · doctor · init · export · mcp (local MCP server)
          │ text only (never audio)
          ▼
   AI agent chosen by the dev (their subscription) + their MCPs (Atlassian, Datadog, GitLab…)

   Orca: optional. Its agents can query meetings through `steno mcp`.
```

**Cost principle**: continuous local transcription; the AI is only called on request (voice or text), once at the end of the meeting, and for correlation if it is enabled.

---

## 2. Technical choices (to be confirmed in phase 0)

| Building block | Choice | Reason | Plan B |
|---|---|---|---|
| Desktop app | **Tauri 2** (Rust + system WKWebView), UI in **Svelte 5** + Vite, no heavy framework | No embedded Chromium: a few tens of MB of RAM instead of several hundred for Electron | 100 % native Rust UI (Slint or egui) if the WebView causes problems |
| Core | Stable Rust, `tokio`, `rusqlite` (`bundled` feature, FTS5 included), `rusqlite_migration`, `serde`, `tracing` | A single language for the app, the CLI and the MCP server | — |
| MCP server | `rmcp` crate (official Rust SDK), stdio transport | All agents speak MCP | Minimal handmade stdio JSON-RPC |
| Audio + ASR | **Swift sidecar** `steno-audio`: Core Audio process taps (`CATapDescription`, macOS 14.2+), `AVAudioEngine` with voice processing, **FluidAudio** (Parakeet TDT v3 + Silero VAD, Apache 2.0) | Best Apple APIs + transcription on the Neural Engine, very little CPU | All in Rust: `objc2-core-audio` for the taps + `whisper-rs` (Metal) or `sherpa-onnx` (Parakeet ONNX) |
| Global shortcuts | `tauri-plugin-global-shortcut` (press/release events) | "Hold to talk" aside mode | — |
| Launch at login | `tauri-plugin-autostart` | The app lives in the menu bar | — |
| Markdown rendering | `marked` + `DOMPurify` (AI-generated content, so always sanitized) + strict CSP | Security | — |
| Diarization of "Eux" | FluidAudio (pyannote), **as post-processing** | Not needed live | Stay with Moi/Eux |

Performance targets (acceptance criteria):
- Idle app (window closed, no meeting): **< 120 MB of RAM** in total, CPU < 1 %.
- In a meeting: average CPU < 25 % on Apple Silicon (ASR runs on the Neural Engine).
- Window opens in < 500 ms; full-text search over 500 meetings < 200 ms.

Points of attention:
- **macOS permissions**: microphone, "System Audio Recording" (`NSAudioCaptureUsageDescription` in the `Info.plist`), Accessibility (Teams window title). Check that the permission granted to `Steno.app` does cover the embedded sidecar (spike S1).
- **Teams**: the new Teams (probable bundle `com.microsoft.teams2`) may output audio through a helper process → tap the whole Teams process tree.
- **Company-managed Mac**: MDM may block the system audio permission (spike S4).

---

## 3. Data — local SQLite

File: `~/Library/Application Support/Steno/steno.db` (permissions 600; rely on FileVault).
`PRAGMA journal_mode=WAL; foreign_keys=ON; busy_timeout=5000`. Versioned migrations.
**A single main writer**: `steno-core` inside the app. The CLI opens the database read-only, and to act it inserts a job into `jobs` (which the app processes); if the app is not running, the CLI runs the job itself.

```sql
CREATE TABLE meetings (
  id TEXT PRIMARY KEY,                 -- e.g. 2026-11-26-1030-daily-orion
  title TEXT, title_source TEXT,       -- teams_window | user | inferred
  started_at TEXT NOT NULL, ended_at TEXT,
  status TEXT NOT NULL,                -- live | ended | processing | done | error
  confidential INTEGER NOT NULL DEFAULT 0,   -- 1 = no AI call for this meeting
  agent_provider TEXT, agent_session_id TEXT,
  correlation TEXT                     -- off | on_demand | auto (copy of the setting at start, editable)
);

CREATE TABLE segments (
  id INTEGER PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL, ts TEXT NOT NULL, t0 REAL, t1 REAL,
  channel TEXT NOT NULL,               -- me | them
  speaker TEXT NOT NULL,               -- Moi | Eux | Eux-1…
  text TEXT NOT NULL,
  private INTEGER NOT NULL DEFAULT 0,  -- aside: excluded from the report
  is_echo INTEGER NOT NULL DEFAULT 0,
  request_id TEXT                      -- if the utterance is a command
);
CREATE VIRTUAL TABLE segments_fts USING fts5(
  text, content='segments', content_rowid='id', tokenize='unicode61 remove_diacritics 2');

CREATE TABLE speakers (meeting_id TEXT, label TEXT, display_name TEXT, PRIMARY KEY (meeting_id, label));
CREATE TABLE notes (id INTEGER PRIMARY KEY, meeting_id TEXT, ts TEXT, text TEXT, source TEXT); -- user | voice

CREATE TABLE requests (
  id TEXT PRIMARY KEY, meeting_id TEXT, created_at TEXT,
  origin TEXT,                         -- voice | text | hotkey | cli
  intent TEXT,                         -- search | check | note | recap | cancel
  query TEXT, context_seconds INTEGER, tier TEXT,   -- quick | search | summary
  status TEXT, started_at TEXT, finished_at TEXT, error TEXT,
  summary TEXT,                        -- 2 lines max (notification)
  answer_md TEXT, tools_used TEXT      -- JSON
);

CREATE TABLE reports (
  meeting_id TEXT PRIMARY KEY, markdown TEXT, generated_at TEXT,
  provider TEXT, model TEXT, correlated INTEGER DEFAULT 0, version INTEGER DEFAULT 1);
CREATE TABLE report_refs (             -- links produced by correlation
  id INTEGER PRIMARY KEY, meeting_id TEXT, kind TEXT,  -- code | jira | confluence | datadog | gitlab | autre
  ref TEXT, title TEXT, url TEXT, verified INTEGER, note TEXT);
CREATE TABLE decisions (id INTEGER PRIMARY KEY, meeting_id TEXT, text TEXT, created_at TEXT);
CREATE TABLE actions (id INTEGER PRIMARY KEY, meeting_id TEXT, owner TEXT, text TEXT, due TEXT,
  status TEXT DEFAULT 'open');         -- open | done
CREATE VIRTUAL TABLE reports_fts USING fts5(markdown, content='reports', content_rowid='rowid',
  tokenize='unicode61 remove_diacritics 2');

CREATE TABLE jobs (
  id INTEGER PRIMARY KEY, type TEXT,   -- request | report | correlate | diarize | export
  meeting_id TEXT, payload TEXT, status TEXT, attempts INTEGER DEFAULT 0,
  created_at TEXT, started_at TEXT, finished_at TEXT, error TEXT);

CREATE TABLE repos (
  id INTEGER PRIMARY KEY, label TEXT, path TEXT UNIQUE,
  use_live INTEGER DEFAULT 1, use_correlation INTEGER DEFAULT 1, exclude_globs TEXT);
CREATE TABLE connectors (               -- MCPs detected in the chosen agent
  id INTEGER PRIMARY KEY, provider TEXT, name TEXT, status TEXT, detected_at TEXT,
  use_live INTEGER DEFAULT 0, use_correlation INTEGER DEFAULT 0,
  write_blockable INTEGER NOT NULL);    -- 0 = cannot be enabled (section 5.3)
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);   -- JSON
```

- **Audio**: never in the database. Temporary buffer in `~/Library/Caches/Steno/`, deleted after processing ("keep audio for N days" setting, 0 by default).
- **Retention**: "delete transcripts after N days" setting (empty = never). Reports can be kept longer.
- **Backup**: daily copy (`VACUUM INTO`) into `backups/`, last 7 kept.
- **Export**: markdown per meeting on demand, or automatic export to a folder (setting, disabled by default).

---

## 4. Component A — `steno-audio` (Swift sidecar)

### 4.1 Role
Audio → text segments only. No business logic, no database access.

### 4.2 Functions
1. **Call detection** in the watched video-call apps (Teams, Slack, Zoom, Webex, Discord, FaceTime, browser for Meet; configurable list, one `call` message per app) (used for the meeting title and to **offer** to start via a notification; listening **never** starts on its own): the Teams process uses the microphone (`kAudioProcessPropertyIsRunningInput`, polled periodically) → `teams_call` event. Title read from the meeting window through the accessibility API (`AXTitle`).
2. **Capture** of two separate streams, resampled to 16 kHz mono: microphone (voice processing enabled, hence echo cancellation) and a tap on the Teams process tree.
3. **VAD + ASR**: an utterance is closed after 0.8 to 1.2 s of silence or 25 s at most; Parakeet v3 transcription; **< 3 s** between the end of the sentence and the segment being emitted.
4. **Vocabulary**: received at start (wake word, service names); injected if the engine supports it, otherwise corrected afterwards with a dictionary.
5. **Aside mode**: while the `aside on` command is active, microphone segments are marked `private`.
6. **Robustness**: device change (AirPods), sleep and wake, Teams restart → recreate the tap without losing the thread.
7. **Wake-word test** (section 6.4): records N seconds of microphone and returns the raw transcript.

### 4.3 Protocol (JSON lines)
Sidecar → core (stdout):
```json
{"type":"ready","version":"0.1.0","asr":"parakeet-tdt-v3"}
{"type":"teams_call","active":true,"title":"Daily ORION"}
{"type":"segment","ts":"2026-11-26T10:41:12+01:00","t0":668.2,"t1":672.9,"channel":"me","text":"Sténo, cherche…","private":false}
{"type":"level","me":-32.5,"them":-24.1}
{"type":"state","state":"listening|paused|stopped"}
{"type":"error","code":"tap_failed","message":"…","recoverable":true}
```
Core → sidecar (stdin):
```json
{"cmd":"start","vocabulary":["Sténo","Atlas"],"silence_ms":1000}
{"cmd":"stop"}
{"cmd":"aside","on":true}
{"cmd":"wake_test","seconds":8}
```

### 4.4 Acceptance criteria
- 30-minute Teams call: complete transcript, Moi and Eux separated, no crash.
- Unplug and replug the headset: recovery in under 5 s.
- No audio file left after processing if retention is 0.

---

## 5. Component B — `steno-core` (Rust): database, jobs, agents

### 5.1 Receiving segments
- Writes segments to the database and feeds the "live" view.
- **Anti-echo**: a `me` segment more than 80 % similar to a `them` segment within a ±2 s window → `is_echo = 1`, ignored.
- **Wake-word detection** on non-echo `me` segments (section 6).

### 5.2 Job queue
- `request`, `report`, `correlate`, `diarize`, `export` jobs, with at most 2 AI jobs in parallel (configurable), a maximum duration, and resumption when the app restarts.
- For a `request`: the prompt contains the request, the last `context_seconds` seconds of transcript (to resolve "that", "this ticket") and the notes.
- **One agent session per meeting** when the agent can resume a session (ID in `meetings.agent_session_id`); otherwise the adapter replays a summary of the previous exchanges.
- Meeting marked `confidential` → **no AI job** is created (transcript only, and only if the user keeps it).
- macOS notifications: « 🔎 Je cherche : … » ("Searching: …") then « ✅ <résumé en 2 lignes> » ("✅ <2-line summary>") (click → opens the answer in the app).

### 5.3 Agent adapters (AI of your choice)
```rust
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> &'static str;                 // "claude-code", "codex", "gemini", "opencode", "custom"
    async fn detect(&self) -> Detection;          // installed? version?
    fn capabilities(&self) -> Capabilities;       // resume_session, stream_json, read_only_files, tool_deny_list, extra_dirs
    async fn list_mcp(&self) -> Result<Vec<McpServer>>;            // for the Connectors screen
    async fn sync_instructions(&self, agent_dir: &Path) -> Result<()>; // generates CLAUDE.md / AGENTS.md / GEMINI.md
    async fn apply_policy(&self, policy: &ReadOnlyPolicy, run_dir: &Path) -> Result<()>;
    async fn run(&self, req: RunRequest, events: Sender<AgentEvent>) -> Result<RunResult>;
}
```

| Adapter | Non-interactive launch | Resume | Instructions | Target status |
|---|---|---|---|---|
| `claude-code` | `claude -p` (`--model`, `--add-dir`, `--output-format stream-json`) | `--session-id` / `--resume` | `CLAUDE.md` + skills | **supported** (reference) |
| `codex` | `codex exec` (read-only sandbox, JSON output) | `codex exec resume` | `AGENTS.md` | supported |
| `gemini` | `gemini -p` | depends on version | `GEMINI.md` | experimental |
| `opencode` | `opencode run` | depends on version | `AGENTS.md` | experimental |
| `custom` | command template entered in the settings | no | file passed in the prompt | generic |

These CLIs' options change fast: the agent implementing them **checks them against the installed version** (`--help`) and records the tested versions in `docs/adapters.md`.

**Model tiers**: the code only knows `quick` (notes, recap), `search` (cross-search) and `summary` (report). Mapping to actual models is done in the settings (empty = the CLI's default model).

**Claude Code and MCP in `-p` mode**: the Claude account's connectors are supposed to be available since v2.1.83 (except with `--bare`), but regressions have been reported → spike S3; plan B: MCPs added locally (`claude mcp add --transport http …`).

### 5.4 Shared read-only policy
`agent/policy.toml`: readable repositories (those checked in the settings), no writable folder, allowed shell commands (`rg`, `git log|show|blame|grep`, `glab mr view|list`…), and forbidden MCP tool patterns (`*send*`, `*post*`, `*create*`, `*update*`, `*edit*`, `*delete*`, `*transition*`, `*comment*`, `*merge*`, `*push*`). Each adapter translates it into its CLI's mechanism.
**Safety rule**: if an adapter cannot forbid write MCP tools, its connectors are marked `write_blockable = 0` and **cannot be enabled** (checkbox greyed out in the UI, with the explanation). The agent only reads and answers on its standard output: `steno-core` is what writes to the database.

### 5.5 Agent instructions (`agent/`)
- A single source: `agent/INSTRUCTIONS.md` + `agent/playbooks/*.md`, generated on the fly into each CLI's format in a temporary run folder.
- Dev profile (role, team), entered in the settings and injected into the instructions.
- Rules: answer in French, briefly when live, **always cite sources** (ticket, Confluence page, `fichier:ligne`, Datadog link), distinguish "verified" from "assumed".
- `recherche-croisee` procedure:
  1. extract entities (`APP-\d+` or a configurable Jira key pattern, services, functions, endpoints, environments, time ranges);
  2. Jira (cited tickets + JQL on the keywords);
  3. Confluence;
  4. code: `rg`, `git log -S`, related MRs via read-only `glab`;
  5. Datadog: logs, monitors and incidents over the relevant time window;
  6. synthesis: 2-line TL;DR, findings per source, contradictions ("the ticket says X, the code does Y"), what remains to be checked.
  - Live budget: at most 15 tool calls (configurable), then offer to "dig deeper after the meeting".
- `note` and `recap` procedures: `quick` tier, no external tool.

### 5.6 Acceptance criteria (testable without audio, with the sample files)
- A text request « où est géré le verrou de créneau et quel ticket en parle ? » ("where is the slot lock handled and which ticket mentions it?") returns an answer in under 90 s, with at least one Jira source and at least one `fichier:ligne` reference.
- A follow-up reuses the meeting's session.
- « Poste ça sur Slack » ("Post this on Slack") is refused; no write tool appears in `tools_used`.
- These tests pass for every adapter marked "supported".

---

## 6. Voice command and wake word

### 6.1 Two modes
1. **Out loud** (participants hear it): « Sténo, cherche les erreurs Datadog sur ms-booking depuis hier. » ("Sténo, look for Datadog errors on ms-booking since yesterday.")
2. **Aside**: mute yourself in Teams and **hold the aside shortcut** (⌃⌥Space by default) while speaking. Sténo captures the microphone independently of Teams, no wake word is needed, and these sentences are marked `private`, hence excluded from the report.

### 6.2 Configurable wake word (Settings → Voice)
- **Wake word**: text field, « Sténo » by default. Users pick their own.
- **Strict mode** (checkbox): requires a prefix (« OK Sténo », « Dis Sténo ») to further reduce false triggers.
- **Accepted variants**: generated automatically (French phonetic variants, common transcription errors) and editable. The word is added to the ASR vocabulary.
- **Words forbidden as variants**: built-in list of words common in dev meetings (« script », « description », « inscrit », « prescrire »…) + the team's vocabulary. A conflicting variant is refused, with the explanation.
- **Checks on entry** (warnings, not blockers):
  - fewer than 2 syllables, or a very common French word → ⚠️ risk of false triggers;
  - **history analysis**: "this word appears 14 times in your last 20 meetings without being a command" → ⚠️;
  - a colleague's first name spotted in the transcripts → ⚠️.
- **« Tester » ("Test") button** (section 6.4).
- Discreet **audio feedback** when a command is detected (configurable: none / system sound / sound in the headset only).

### 6.3 Detection rules (out-loud mode)
- **Only on the `me` channel** (and excluding echo): a participant can never trigger Sténo.
- The wake word (or prefix + word in strict mode) must be **within the first 2 words of the sentence**.
- It must be followed by a recognized verb or a pause, then the request:
  - `cherche|recherche|regarde|trouve` → `search`
  - `vérifie|confirme` → `check`
  - `note|ajoute une action|retiens` → `note` (`quick` tier, no search)
  - `résume|récap` → `recap`
  - `annule` → `cancel` (latest pending request)
  - `arrête l'écoute|termine la réunion|fin de la réunion` → `stop` (stops listening and starts the report)
  - The verbs can be edited in the settings (grammar file in the database).
- End of command: consecutive `me` sentences less than 2 s apart are merged, until 1.5 s of silence.
- **Never any speech synthesis in the Teams audio.**

### 6.4 « Tester » ("Test") button
1. The user says « <mot d'appel>, cherche quelque chose » ("<wake word>, search for something") 5 times → detection rate shown (target: 5/5).
2. They read a short trap text containing the forbidden words and similar-sounding words → target: 0 triggers.
3. If the rate is too low, the app offers to add the missed transcriptions as variants.

### 6.5 Acceptance criteria
- With the default word, 20 commands read aloud: at least 18 detected with the right intent.
- 1 h of meeting audio containing « script », « description », « inscrit »: **0 false triggers**.
- The same command said by a participant (`them` channel): 0 triggers. With speakers instead of a headset: 0 echo-induced triggers.
- Changing the wake word in the settings takes effect without a restart.

---

## 7. Report and correlation

### 7.1 Report (end of meeting, always, except for confidential meetings)
Triggered when the meeting moves to `ended`, i.e. **only when the user asks to stop** (status bar menu, shortcut, app, or « Sténo, arrête l'écoute » ("Sténo, stop listening")). The end of the Teams call does not stop listening.
1. Offline diarization of the `them` channel (Eux-1, Eux-2…). Claude suggests names when they are spoken (« merci Marie » ("thanks Marie")); the user validates in the app.
2. Agent, `summary` tier, **without external tools** (fast, does not use up search calls):
   ```
   # <Titre> — <date>
   ## En bref (3 lignes)
   ## Décisions
   ## Actions — qui / quoi / échéance
   ## Questions ouvertes
   ## Points techniques évoqués
   ## Recherches faites pendant la réunion
   ## Mes notes (fusionnées, comme dans Granola)
   ```
   `private` sentences are never used.
3. Decisions and actions extracted into their tables; any drafts (proposed Jira comments…) stored in the app, **never published automatically**.
4. Audio deleted according to retention; « 📝 Compte rendu prêt » ("Report ready") notification.

### 7.2 Correlation with the codebase and connectors (configurable)
- **Global mode** (Settings → Correlation): `Désactivée` (off) / `À la demande` (on demand; « Corréler » ("Correlate") button on the report) / `Automatique` (automatic; runs right after the report).
- **Sources**: checkboxes per repository and per connector (`use_correlation`), independent of the sources used live (`use_live`).
- **Exclusions**: never correlate meetings whose title contains certain words (default: « 1:1 », « RH », « entretien », « perso »); never for a confidential meeting.
- **Budget**: maximum number of tool calls per correlation (30 by default).
- **What the `correlate` job does**: takes the "Points techniques évoqués" (technical points raised) and the open questions, applies the `recherche-croisee` procedure to the checked sources, fills `report_refs` (each link marked verified or not), then appends to the report a **« Liens avec le code et les outils »** ("Links to code and tools") section with alerts ("decision X contradicts the current behaviour of `fichier:ligne`"). The report moves to `version + 1`; the previous version remains viewable.

### 7.3 Acceptance criteria
- 45-minute sample transcript: report in under 2 min, at least 1 decision and 1 action correctly attributed, no `private` content.
- Correlation disabled: no `correlate` job is created; on demand: only after a click; exclusions respected (tested on a « 1:1 » meeting).

---

## 8. Desktop app (Tauri 2)

### 8.1 Menu bar (always present)
Icon in the macOS status bar (next to the clock), native menu on click: status (« ● Écoute en cours — titre » / « Appel Teams détecté : titre » / « En attente » — "● Listening — title" / "Teams call detected: title" / "Idle"), **« Démarrer / Arrêter l'écoute »** (Start / Stop listening), « Réunion confidentielle » (Confidential meeting), « Demande rapide… » (Quick request…; panel: request field, last 5 answers), « Voir la transcription en direct » (View live transcript), « Ouvrir Sténo » (Open Sténo), « Réglages… » (Settings…), « Quitter » (Quit). A "●" is shown next to the icon while listening.

### 8.2 Main window
- **Meetings**: list by date with full-text search (FTS5 over transcripts and reports), filters (period, correlated or not, with open actions).
- **Meeting detail**, in tabs:
  - **Report**: rendered markdown, versions, « Corréler » (Correlate) and « Régénérer » (Regenerate) buttons, markdown export or copy;
  - **Transcript**: timestamped, colours per speaker, speaker renaming (Eux-1 → Marie), private sentences hidden by default;
  - **Requests**: each request with its answer, its sources and the tools used;
  - **Notes**: editable.
  - Actions: rename, mark confidential, delete (with confirmation).
- **Live**: scrolling transcript, « Demander à Sténo » (Ask Sténo) field, feed of pending requests.
- **Search**: full text across all meetings, plus « Demander à l'IA » (Ask the AI; a question about the history, via the agent and the Sténo MCP server).
- **Decisions & actions**: all actions from all meetings, "done" checkbox, filters by person and by date.

### 8.3 Settings
| Section | Content |
|---|---|
| **AI agent** | Agent detected among those installed (list), model per tier (`quick`, `search`, `summary`), command template for `custom`, « Tester l'agent » (Test the agent) button, « Autoriser l'utilisation d'une clé API » (Allow using an API key; disabled by default) |
| **Repositories** | Add a folder (picker), label, used live / for correlation, exclusions (globs) |
| **Connectors** | List of the chosen agent's MCPs (via `list_mcp`), status, live / correlation checkboxes, ❌ greyed out if the agent cannot block their write tools, « Rafraîchir » (Refresh) button |
| **Correlation** | Mode (off / on demand / automatic), budget, exclusion words |
| **Voice** | Wake word, strict mode, variants, verbs, audio feedback, « Tester » (Test) button (section 6) |
| **Audio & transcription** | Microphone, automatic Teams detection, end-of-sentence silence, vocabulary (service names, team), shortcuts (start/stop, aside, quick note) |
| **Privacy** | Audio and transcript retention, automatic export, excluded meetings, « Tout supprimer » (Delete everything) |
| **Profile** | Role, team, Jira key pattern |
| **Import / export** | Export / import settings as TOML (to share a starter config within the team, without paths or personal data) |

### 8.4 UI constraints
No heavy framework; front-end bundle < 300 KB gzip; light and dark theme following the system; keyboard navigation; no network request from the WebView (CSP `default-src 'self'`).

---

## 9. CLI, MCP server and Orca (optional)

- `steno ask "qu'a-t-on décidé sur l'export Brindille ?"`: searches the history, then possibly runs a cross-search.
- `steno doctor`: macOS permissions, Teams, sidecar, agent and version, MCPs visible in non-interactive mode, and an **automatic test of the read-only policy** (« poste sur Slack » ("post on Slack") must fail) → ✅ / ⚠️ / ❌ with the steps to follow.
- `steno init`: first-run setup from the command line (equivalent of the app's wizard).
- `steno export <id|--all> --format md`.
- `steno mcp`: **local MCP server (stdio, read-only)** for any agent, including in Orca:
  - `search_meetings(query, from?, to?)`, `get_meeting(id)`, `get_transcript(id, from_ts?, to_ts?)` (without private sentences), `get_report(id)`, `list_decisions(since?)`, `list_actions(status?, owner?)`.
  - To use it in an agent: `claude mcp add steno -- steno mcp` (or the CLI's equivalent).
- **Orca is no longer required**; for those who use it, `steno mcp` gives their agents access to meetings.

---

## 10. Phases

### Phase 0 — Spikes (1 d): **blocking**
| Spike | Deliverable | Criterion |
|---|---|---|
| S1 Capture | minimal sidecar that writes `me.wav` + `them.wav` during a real Teams call, **launched from an empty Tauri app** | two audible tracks, permissions granted to the app and valid for the sidecar |
| S2 ASR | transcription of S1 with FluidAudio/Parakeet v3 (compared with WhisperKit) | readable French, technical names understood, < 1 s per sentence |
| S3 Agents | ask Claude Code, then Codex, in non-interactive mode, to list their MCP tools; try a forbidden write tool | MCPs visible; writing blocked; otherwise plan B documented |
| S4 Workstation | microphone, system audio and accessibility permissions on the work Mac | granted; otherwise stop and tell the user |
| S5 Footprint | empty Tauri app + menu bar, idle RAM measurement | < 120 MB |

### Phase 1 — Foundation
`steno-core` (schema, migrations, job queue), sidecar (capture + transcription), minimal "live" view.

### Phase 2 — Report + history
End-of-meeting report, Meetings / Detail screens, full-text search.

### Phase 3 — Agents and cross-search
`claude-code` then `codex` adapters, read-only policy, typed requests, AI agent / Repositories / Connectors screens.

### Phase 4 — Voice
Wake-word detection, aside mode, Voice screen with the « Tester » (Test) button.

### Phase 5 — Correlation + polish
`correlate` job, Correlation screen, decisions and actions, `ask` / `export` CLI, `steno mcp` server, settings import/export.

### Phase 6 — Team
`make install`, first-run wizard, complete `steno doctor`, `gemini` and `opencode` adapters (experimental), README.

---

## 11. Split into agents (after phase 0)

Shared prerequisite, done by the first agent: `docs/contracts.md` (SQL schema + sidecar protocol + `AgentAdapter` trait) and `fixtures/` with a 45-minute **synthetic French transcript** (fictional daily: tickets, an `ms-*` service, a decision, two actions, a voice command, a private sentence, trap words such as « script »).

| Worktree | Agent | Scope | Testable without |
|---|---|---|---|
| `steno-audio` | A | `audio/` (Swift sidecar) | database, UI |
| `steno-core` | B | `crates/steno-core`: database, jobs, adapters, policy, wake word | audio (sample files) |
| `steno-post` | C | report, correlation, `crates/steno-cli` (ask, export, mcp, doctor) | audio, UI |
| `steno-app` | D | `app/` (Tauri + Svelte): menu bar, screens, settings | audio (sample data in the database) |

Directory layout:
```
steno/
  PLAN.md
  docs/{contracts.md, spikes.md, adapters.md}
  audio/StenoAudio/            # Swift package, sidecar executable
  crates/steno-core/           # Rust lib: db, jobs, agents/, policy, wake/
  crates/steno-cli/            # `steno` bin: ask, doctor, init, export, mcp
  app/src-tauri/                # Rust (depends on steno-core), embeds the sidecar
  app/ui/                       # Svelte 5 + Vite
  agent/                        # INSTRUCTIONS.md, playbooks/, policy.toml
  fixtures/
  Makefile
```

---

## 12. Costs and quota

- Audio, transcription, diarization, database, app: **local, €0**.
- AI: only through the official CLI of the chosen agent, hence within each dev's subscription. Orders of magnitude: 1 h of transcript ≈ 15,000 tokens; what consumes the most is **cross-searches** and **correlation**, hence their budgets and the "on demand" mode.
- Safeguards: `quick` tier for `note` and `recap`, report without tools, at most 2 AI jobs in parallel, no continuous summarization.
- API key: never, unless explicitly enabled in the settings.
- Distribution: local build with ad hoc signing, so no paid Apple developer account.

---

## 13. Risks and safeguards

| Risk | Mitigation |
|---|---|
| MDM blocks the system audio permission | Spike S4 first; stop and tell the user if it is blocked |
| The permission granted to the app does not cover the sidecar | Spike S1; plan B: Rust capture (`objc2-core-audio`) in the main process |
| Teams outputs audio through a helper process | Tap the whole Teams process tree; plan B: global tap excluding Sténo |
| Echo (speakers) | AEC + deduplication + headset recommended |
| False triggers | Wake word at the start of the sentence + verb, strict mode, forbidden words, history analysis, `me` channel only, tests |
| An agent CLI changes its options | Isolated adapters, tested versions recorded, `steno doctor` reports any mismatch |
| An agent cannot block write MCPs | Connectors cannot be enabled for that adapter |
| The agent writes externally | Explicit-deny policy + automatic test in `doctor` and in CI |
| Malicious AI content in the UI | Sanitized markdown (DOMPurify) + strict CSP |
| Privacy | Audio never sent nor stored long-term; local database at 600; confidential meetings without AI; configurable retention |
| Consent | Inform participants that the meeting is being transcribed (GDPR); for company-wide use: security / GDPR review (and probably the works council (CSE)) |

---

## 14. Use by other devs (building on their machine)

Goal: clone, build and be up and running in under 15 minutes.

- **Prerequisites** (README): macOS 14.2+ on Apple Silicon, Xcode (for the Swift sidecar), stable Rust (`rustup`), Node 22 (only to build the UI), at least one agent CLI installed and signed in.
- **`make install`**: builds the sidecar, the UI and the Tauri app, applies ad hoc signing, copies `Steno.app` into `~/Applications`, installs the `steno` CLI, downloads the audio models.
- **First-run wizard** (in the app, or `steno init`):
  1. macOS permissions, guided step by step;
  2. choice of agent among those detected, then a test;
  3. repositories to make readable;
  4. connectors to enable (only those whose writes can be blocked);
  5. wake word, then a test;
  6. correlation mode (default: on demand).
- **Team config**: a TOML file of starter settings, without paths or personal data (service vocabulary, verbs, exclusions), importable from the Settings screen.
- **Update**: `git pull && make install`; automatic database migrations, with a backup right before.
