# Releasing a version

Users build Sténo from source. A version is therefore a **signed tag** `vX.Y.Z` on `main`:
the app detects it (GitHub public API, every 6 hours) and offers to install it. Installing
moves the user's clone to that tag and then runs `make install`.

1. On a branch, change the version number everywhere (the script uses BSD `sed`: run it on
   macOS):
   ```sh
   scripts/bump-version.sh 0.2.0
   git commit -am "Version 0.2.0"
   ```
   Open a pull request; once CI is green, merge it into `main`.
2. Create and push the tag (signed, administrators only):
   ```sh
   git checkout main && git pull
   git tag -s v0.2.0 -m "Sténo 0.2.0"
   git push origin v0.2.0
   ```
3. The **Release** workflow checks that the tag matches the version in the code and creates
   the GitHub release, with notes generated from the pull requests. Edit them if needed: they
   are shown in Settings → Updates.

Rules:
- `main` only accepts pull requests, with green CI and signed commits.
- Tags are signed with the SSH release key. The real checks are the **Release** workflow (keys
  read from `main`, tagged commit on `main`) and the app (keys compiled in, tag on `main`):
  `git verify-tag` against `crates/steno-core/release-signers`. The tag ruleset's
  `required_signatures` rule does not protect much: administrators bypass the ruleset and are
  the only ones allowed to create tags. To change keys: first release, signed with the old key,
  a version that adds the new key to that file on `main`, then sign the following versions
  with the new one.
- A published tag is never moved or deleted: if something is wrong, release the next version.
- `X.Y.Z` versions only; a pre-release (`v0.3.0-rc1`) is published automatically as a
  "pre-release" by the workflow and is never offered to users.
