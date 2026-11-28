# Security

Sténo listens to your microphone and to the audio of your calls: security and privacy are at
the heart of the project. Please report any vulnerability responsibly.

## Reporting a vulnerability

- **Do not disclose** the vulnerability in a public issue.
- Use GitHub's private reporting (the repository's **Security → Report a vulnerability** tab)
  or write to **dev@nakile.fr** with the subject "Sténo security".
- Describe the vulnerability, the version (`steno --version` or Settings), the steps to
  reproduce it, and its impact.

We acknowledge reports within 7 days and aim for a fix within 30 days depending on severity.
You will be credited in the release notes if you wish.

## Supported versions

Only the latest version of the main branch receives security fixes.

## Security model

### What stays on your computer

- **Audio**: captured and transcribed locally (Neural Engine / GPU on a Mac, CPU on Linux).
  Never sent over the network. Temporary files (`~/Library/Caches/Steno/<meeting>` on macOS,
  `~/.cache/Steno/<meeting>` on Linux) are deleted after the summary, unless you chose a
  retention period (0 days by default).
- **Data**: local SQLite database (`~/Library/Application Support/Steno/steno.db` on macOS,
  `~/.local/share/Steno/steno.db` on Linux, mode 600), daily local backup (last 7). No Sténo
  server, no telemetry.
- **Documents attached** to a meeting's context: their text is extracted locally and kept in
  the database; the original file is not kept.

### What leaves your computer, and to whom

- Only **text** (transcript excerpts, notes, context, text of attached documents) is sent, and
  only to the **AI agent CLI you chose**, which forwards it to its vendor under its own terms.
  This happens: when you ask (spoken or typed command), at the end of the meeting (summary),
  and for correlation if it is enabled.
- **Confidential meeting**: no AI call at all, neither search nor summary; its excerpts are
  never used as context for questions about past meetings, and the local MCP server
  (`steno mcp`, opened read-only) neither lists nor returns it.
- **Asides**: the private transcript and the requests made in an aside appear neither in the
  summary, nor in the export, nor in `steno mcp`, nor in the menu bar's recent requests; they
  do not resume the meeting's agent session, are never replayed to a public request, a note
  dictated in an aside stays private, and an aside's notifications show no text.
- **Switching to confidential mid-meeting**: the meeting's pending jobs (summary, correlation,
  export, requests) are cancelled, running agent runs are stopped, and automatic export never
  writes a confidential meeting.
- The **audio models** are downloaded from Hugging Face on first use; no data is sent during
  this download.
- **Updates**: at startup and then every 6 hours, Sténo reads the latest published version
  from GitHub's public API (`api.github.com`), with no authentication and no personal data.
  This can be turned off in Settings → Updates. Installing fetches the version's tag into your
  clone (`git fetch`) and rebuilds locally, only if the clone tracks the official repository,
  the version is exactly `X.Y.Z`, the tag carries this name in its header and is SSH-signed by
  the release key known to the app (`crates/steno-core/release-signers`, fingerprint checked,
  GPG disabled), its code is at this version, and it is on `main` and newer than the sources;
  exactly that commit, unmodified, is built. An imported team setting cannot enable automatic
  install, API keys or automatic correlation, change the agent or remove an excluded meeting.

### The agent only reads

- The agent runs read-only: write tools denied, shell commands limited to an allowlist, reads
  limited to the repositories you ticked.
- For Claude Code, every tool call goes through a hook (`steno policy-hook`) that denies write
  MCP tools (`*send*`, `*create*`, `*comment*`, `executeWrite`…) and those of a server you did
  not tick (exact name), options that run a program or write a file (`rg --pre`,
  `find -exec` / `-delete`, `sort -o`, `git grep -O`, `git branch -d`…) or follow symbolic
  links (`rg -L`, `grep -R`, `find -L`…), shell expansions and
  redirections (`;`, `&`, `>`, `$`, `` ` ``, `(`…), and paths that leave the repositories once
  symbolic links are resolved. Denial is exit code 2; an internal hook error also denies.
  Without this hook, Claude Code is not started.
- Files excluded in a repository's settings (`.env*`, `*.pem`…) are a filter on direct file
  reads (a file that is read, named in a command or matched by a `Glob` pattern), not a
  barrier: a recursive search or the Git history (`git log -p`, `git diff`) can still show
  their content, shell wildcards (`*.pe?`, `key.pe[m]`) are not matched against them, and only
  Claude Code applies them (Codex and OpenCode have no per-file filter).
- A symbolic link committed inside a ticked repository that points outside it can still be
  reached through a shell wildcard: only tick repositories you trust.
- Data folders are mode 700; prompt and run settings files are mode 600.
- An agent that cannot block its MCP tools has its connectors disabled.
- API keys whose mere presence makes an agent bill the API instead of your subscription
  (Anthropic, OpenAI, Gemini, xAI, OpenRouter, and OpenCode's providers) are not passed to the
  agent, unless explicitly enabled in the settings; an imported team setting cannot enable
  this. The list is best-effort. Cloud credentials (AWS, Google service account, Azure) are
  left in place so Bedrock or Vertex setups keep working, and your custom command receives
  your full environment.
- Meeting content is always presented to the agent as **data**, never as instructions
  (protection against instruction injection by a participant), in blocks that the text itself
  cannot close.

### Local development API

`steno serve` (tests and development) only listens on `127.0.0.1` and refuses requests from
another origin or another host name (DNS rebinding). In the app, commands that edit settings,
meetings, actions, repositories or connectors, delete data, install an update, write a file or
request a permission are only accepted from the main window. Besides read-only commands, the
menu bar panel can start, pause and stop listening, mute the microphone, make the current
meeting confidential, ask about the current meeting, cancel a request and open the main window
(full list in `docs/contracts.md` §5).

### macOS permissions

| Permission | Use |
|---|---|
| Microphone | Transcribe your voice and detect the wake word |
| System audio ("Screen & System Audio Recording") | Hear the other participants, only in the call app |
| Accessibility (optional) | Read the title of the current call |
| Calendars (optional) | Read your upcoming meetings and their invitees (read-only, nothing is modified); invitee names are sent to the agent for the summary |

To offer the "system audio" permission in the setup assistant, Sténo uses a private macOS API
(`TCC.framework`, loaded dynamically), like other audio capture tools. It is only used to
request or check this permission.

On Linux there is no system permission: the microphone and the call app's audio go through
your session's sound server (PipeWire or PulseAudio).

### Signing

The app is built on your machine, without an Apple developer account. On macOS,
`make install` creates once, in your login keychain, a self-signed "Steno Local Signing"
certificate (`scripts/local-signing.sh`) and signs the app with it, so macOS keeps its
permissions from one version to the next. This certificate never leaves your Mac and is only
used for apps built there; you can delete it in Keychain Access (the signature then reverts
to "ad hoc"). Build from a trusted source and review the repository before `make install`.
