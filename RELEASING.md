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
4. The same workflow builds and attaches the prebuilt packages, each with a GitHub artifact
   attestation (`<file>.sigstore.json`):
   - `Steno-X.Y.Z-macos-arm64.tar.gz`: `Steno.app` (with the Swift sidecar) and the `steno` CLI,
     signed ad hoc only. The installer signs it again on the user's Mac with the local
     "Steno Local Signing" identity, so macOS keeps its permissions across versions;
   - `steno_X.Y.Z_amd64.deb`: the Debian / Ubuntu package.

   Check that both files are on the release before announcing it. Anyone can verify where a
   file comes from:
   ```sh
   gh attestation verify Steno-0.2.0-macos-arm64.tar.gz --repo NakileSolutions/Steno \
     --signer-workflow NakileSolutions/Steno/.github/workflows/release.yml \
     --source-ref refs/tags/v0.2.0
   ```

Rules:
- `main` only accepts pull requests, with green CI and signed commits.
- A published tag is never moved or deleted: if something is wrong, release the next version.
- `X.Y.Z` versions only; a pre-release (`v0.3.0-rc1`, marked "pre-release") is not offered to
  users.
