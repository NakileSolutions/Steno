---
name: migration
description: Change Sténo's SQLite schema or a setting (new field, new default value, rename) without breaking existing installs. Use for "add a column", "new setting", "change the default".
---

# SQLite schema and settings

Users update in place (`make install` on their clone): their database
`~/Library/Application Support/Steno/steno.db` and their settings must keep working, with
no data loss.

## SQLite schema

1. Create `crates/steno-core/src/db/migrations/NNN_topic.sql` (next number, 3 digits).
   **Never edit a migration that has been merged into `main`.**
2. Append it to the `include_str!` list in `crates/steno-core/src/db/mod.rs`.
3. Prefer additive migrations (`ALTER TABLE … ADD COLUMN … DEFAULT …`). Think about indexes
   and the FTS5 table if a text column must be searchable.
4. Update the queries (`db/queries.rs`), the types, and `docs/contracts.md` §1.
5. Test: a test that opens a database, applies every migration and checks the new column;
   plus the perf test if search is affected
   (`cargo test -p steno-core --release --test perf -- --ignored`).

## Settings

Typed settings live in `crates/steno-core/src/settings.rs` and are described in
`docs/contracts.md` §4.

- **New setting**: default via `serde(default)` so older files still load; add it to the UI
  (`app/ui/src/routes/settings/`) with its translation.
- **Changed default**: already-saved settings keep the old value (real case: the wake word
  stayed "Scribe" after the rename). If the old value must change, write an explicit upgrade
  step and test it.
- **Rename or format change**: keep reading the old format (see the "ancien format encore lu"
  test in `settings.rs`).
- Team settings (TOML import/export): never let personal data in
  (test `cli::reglages_d_equipe_sans_donnees_perso`).
