---
name: release
description: Prepare a new Sténo version (X.Y.Z everywhere, release PR, signed tag). Use for "ship 0.3.0", "prepare a release", "bump the version".
---

# Release a version

Reference procedure: `RELEASING.md`. Tagging is restricted to administrators: an agent
prepares the PR and stops there, unless the maintainer explicitly asks for more.

1. Start from an up-to-date `main`, on a `version-X.Y.Z` branch.
2. `scripts/bump-version.sh X.Y.Z` (Cargo workspace including the Linux sidecar, Tauri, UI, Swift
   sidecar, lockfiles; the script uses BSD `sed -i ''`, so run it on macOS). Check the diff: only version numbers and lockfiles.
3. Signed commit "Version X.Y.Z", then a PR listing the user-visible changes since the
   previous version (`git log --oneline vPREVIOUS..`). The maintainer merges.
4. After the merge, by an administrator:
   `git checkout main && git pull && git tag -s vX.Y.Z -m "Sténo X.Y.Z" && git push origin vX.Y.Z`.
   The release workflow (`.github/workflows/release.yml`) checks the tag and creates the GitHub release.
5. A published tag is never moved or deleted: if something is wrong, release the next version.

Reminder: users install from source, so a broken release blocks their automatic update.
Run `make test` (and `make install` on macOS, `make deb` on Linux) before opening the PR.
