# Contributing to Sténo

Thanks for helping. Sténo is a public repository and a privacy-sensitive tool, so a few rules
are strict. The full project rules, also for coding agents, are in [`AGENTS.md`](AGENTS.md).

## Before you start

- For anything larger than a small fix, **open an issue first** to agree on the approach.
- Read [`docs/contracts.md`](docs/contracts.md) if you touch an interface: database schema,
  sidecar protocol, agent adapters, settings or the app API.

## Non-negotiable rules

1. **Made-up data only.** No real company, customer, colleague, project or internal tool names,
   no personal paths, no keys or tokens: not in code, tests, fixtures, screenshots or examples.
2. **Sténo never publishes anything.** The agent only reads. Do not weaken the read-only policy
   (`crates/steno-core/src/policy.rs`, `agent/policy.toml`) unless an issue explicitly asks for
   it, with tests.
3. **Audio never leaves the computer.** No telemetry, no Sténo server.
4. **Listening never starts or stops on its own.**

## Conventions

- **English** for everything technical: code comments, test names, logs, commits, pull
  requests, docs. Some older French text is being migrated area by area; do not translate it as
  a side effect of an unrelated change.
- **Bilingual UI, French first**: `t('Texte en français')` with the English translation in
  `app/ui/src/lib/i18n/en/`, and `tr(fr, en)` in the core. Every user-facing string needs both.
- **API commands** live in four places that must stay in sync (contract, `api.rs`, Tauri
  commands, UI): see `.claude/skills/api-command/SKILL.md`.
- **Database changes** go through a new numbered migration; never edit a merged one.
- **Docs in the same pull request** as the change (README, `docs/contracts.md`).
- **Sidecar protocol** changes land in both sidecars (Swift and Linux) and in
  `docs/contracts.md` §2.

## Tests

```bash
make test                          # everything that runs on your platform
cargo test --workspace             # Rust (needs `make sidecar-placeholder` and the UI built)
cd app/ui && npm run check && npm run test:unit
cd app/ui && npm run test:e2e      # Playwright (first time: npx playwright install chromium)
cd audio/StenoAudio && swift test  # macOS sidecar, macOS only
```

Run at least the tests for the area you touched. Never disable a test to get green CI.

## Pull requests

- Branch from `main`; keep each pull request to one topic.
- Title in English, saying what changes for the user. Description: **What**, **Why**,
  **How it was tested** (commands you ran, manual checks on macOS or Linux).
- **Signed commits are required** (GPG or SSH): the `main` ruleset rejects unsigned commits.
  See [GitHub's guide to signing commits](https://docs.github.com/authentication/managing-commit-signature-verification/signing-commits).
- `main` requires the CI check to pass on a branch **up to date** with `main`: rebase on
  `main` when GitHub asks for it.
- The maintainer reviews and merges.

## Security

Please report vulnerabilities privately, as described in [`SECURITY.md`](SECURITY.md), not in
a public issue.
