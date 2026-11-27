---
name: api-command
description: Add, rename or change a Sténo API command (UI ↔ core) while keeping the contract, the HTTP dispatcher, the Tauri app, the TypeScript client and the fake backend in sync. Use whenever a screen needs new data or a new action from the core.
---

# API command

Every command is exposed twice with the same list: by the Tauri app (`invoke`) and by
`steno serve` (HTTP, used by the e2e tests). Two tests check they match:
`api::tests::toutes_les_commandes_du_contrat_sont_connues` (steno-core) and
`tests::commandes_tauri_conformes_au_contrat` (Tauri app).

Reference example to follow: `start_meeting` (`grep -rn start_meeting crates app docs`).

## Steps

1. **Contract** — `docs/contracts.md` §5, "Commandes" table: one row
   `| \`my_command\` | arguments | return type |`. New type → §5 "Types (TypeScript)".
   New event → §5 "Événements".
2. **Core** — the logic in `crates/steno-core/src/` (an `async` method on `Core`).
3. **Dispatcher** — `crates/steno-core/src/api.rs`: add the name to `COMMANDS` and an arm in
   `dispatch` (`snake_case` arguments, `a.opt(…)` for optional ones). If the command has side
   effects or required arguments, add it to the test's exclusion list.
4. **Tauri** — `app/src-tauri/src/commands.rs`: a `#[tauri::command] pub async fn` with the
   same name and arguments; register it in `generate_handler![…]` in
   `app/src-tauri/src/lib.rs`. Never a synchronous command that blocks the main thread.
5. **Client** — `app/ui/src/lib/api.ts`: a typed method calling `call('my_command', {…})`
   (`snake_case` keys); types in `app/ui/src/lib/types.ts`.
6. **Fake backend** — `app/ui/src/lib/mock.ts`: a plausible handler (made-up data), otherwise
   the mock UI and the `maquette` Playwright tests break.
7. **Strings** — core error messages via `tr(fr, en)`; UI strings via `t('…')` plus the
   English translation in `app/ui/src/lib/i18n/en/`.
8. **Tests** — a Rust test for the behavior; if a screen uses it, a Vitest or Playwright test.
   Then `cargo test --workspace` and `cd app/ui && npm run check && npm run test:unit`.

Renaming or removing a command: the same six files, plus a global search
(`grep -rn old_command crates app docs`).
