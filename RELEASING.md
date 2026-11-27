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
- A published tag is never moved or deleted: if something is wrong, release the next version.
- `X.Y.Z` versions only; a pre-release (`v0.3.0-rc1`, marked "pre-release") is not offered to
  users.
