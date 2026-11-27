---
name: prepare-pr
description: Checks to run before opening or updating a pull request on Sténo — tests for the touched area, made-up data only, translations, API contract, docs. Use for "prepare the PR", "open a PR", "is this ready to push?".
---

# Prepare a pull request

1. **Up-to-date base**: `git fetch origin && git rebase origin/main`. Resolve conflicts without
   dropping other people's work (never `--force` on `main`).
2. **Scope**: `git diff origin/main --stat`. Remove anything off-topic (reformatting,
   generated files, `.dev-home/`, `app/ui/dist/`, unintended screenshots).
3. **Tests for the touched area** (see `git diff --name-only origin/main`):
   - `crates/` or `app/src-tauri/` → `make sidecar-placeholder && (cd app/ui && npm run build) && cargo test --workspace`;
     if search or the database changed: `cargo test -p steno-core --release --test perf -- --ignored`;
   - `app/ui/` → `cd app/ui && npm run check && npm run test:unit`, then `npm run test:e2e` if a screen or the API changed;
   - `audio/StenoAudio/` → `cd audio/StenoAudio && swift test` (not covered by CI: say so in the PR);
   - `crates/steno-audio-linux/` → covered by `cargo test --workspace`; for capture changes, also try it on a real Linux desktop (`make install`);
   - sidecar protocol changed → both sidecars and `docs/contracts.md` §2;
   - when in doubt: `make test`.
   If a test fails, fix it or report it explicitly; never disable a test to get green.
4. **Public repository**: review the diff for real names (company, customers, colleagues,
   internal projects), personal paths (`/Users/…`), email addresses, tokens.
   `git diff origin/main | grep -nE '/Users/|@[a-z0-9-]+\.(com|fr)|token|secret|apikey'`.
5. **Consistency**:
   - new UI text → French source + English translation in `app/ui/src/lib/i18n/en/`;
   - API command added or changed → `api-command` skill;
   - schema or setting changed → `migration` skill;
   - user-visible behavior changed → `README.md` and `README.fr.md`; interface changed → `docs/contracts.md`.
6. **Signed commits**: `git log --show-signature origin/main..` (`main` rejects unsigned commits).
7. **PR**: `gh pr create` with an English, user-facing title and a description with
   *What*, *Why*, *How it was tested* (commands run, manual checks on macOS or Linux), and *Watch out
   for* (migration, macOS permissions, sidecar untested in CI…). Do not merge: the maintainer
   does.
