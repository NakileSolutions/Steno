#!/bin/sh
# Installs a prebuilt Sténo release, after checking that it was built by this repository's
# Release workflow from the matching tag (GitHub artifact attestation).
#
#   sh install.sh            # latest release
#   sh install.sh 0.4.0      # a given version
#
# macOS (Apple Silicon): Steno.app goes to /Applications (APP_DIR), and is signed on this Mac
# with the local "Steno Local Signing" identity, so macOS keeps its permissions across
# versions. The `steno` CLI goes to ~/.local/bin (BIN_DIR).
# Debian / Ubuntu (amd64): installs the .deb with apt (asks for sudo).
#
# Needs curl and the GitHub CLI (`gh`) to verify the download. Nothing is installed if the
# verification fails.
set -eu

REPO=NakileSolutions/Steno
APP_DIR=${APP_DIR:-/Applications}
BIN_DIR=${BIN_DIR:-$HOME/.local/bin}

say() { printf '%s\n' "$*"; }
fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null || fail "curl is missing."
command -v gh >/dev/null || fail "the GitHub CLI (gh) is needed to verify the download.
  macOS: brew install gh    Debian / Ubuntu: sudo apt install gh    Other: https://cli.github.com"

# Version: argument, otherwise the latest published release.
VERSION=${1:-}
if [ -z "$VERSION" ]; then
  VERSION=$(curl -fsSL -H "Accept: application/vnd.github+json" "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p' | head -1)
  [ -n "$VERSION" ] || fail "could not find the latest release of $REPO."
fi
VERSION=${VERSION#v}
echo "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || fail "invalid version: $VERSION (expected X.Y.Z)"

OS=$(uname -s)
ARCH=$(uname -m)
case "$OS" in
  Darwin)
    [ "$ARCH" = arm64 ] || fail "prebuilt packages are for Apple Silicon Macs only; on this Mac, build from source (README)."
    FILE="Steno-$VERSION-macos-arm64.tar.gz" ;;
  Linux)
    [ "$ARCH" = x86_64 ] || fail "prebuilt packages are for amd64 only; on this machine, build from source (README)."
    command -v apt-get >/dev/null || fail "the prebuilt package is a .deb (Debian / Ubuntu); on this system, build from source (README)."
    FILE="steno_${VERSION}_amd64.deb" ;;
  *) fail "unsupported system: $OS" ;;
esac

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
URL="https://github.com/$REPO/releases/download/v$VERSION"
say "Downloading Sténo ${VERSION}…"
curl -fL --retry 3 --progress-bar -o "$TMP/$FILE" "$URL/$FILE" || fail "download failed: $URL/$FILE"
curl -fsSL --retry 3 -o "$TMP/$FILE.sigstore.json" "$URL/$FILE.sigstore.json" \
  || fail "the attestation of $FILE is missing from the release: not installing."

say "Verifying that it was built by $REPO's Release workflow from tag v${VERSION}…"
gh attestation verify "$TMP/$FILE" --bundle "$TMP/$FILE.sigstore.json" --repo "$REPO" \
  --signer-workflow "$REPO/.github/workflows/release.yml" --source-ref "refs/tags/v$VERSION" >/dev/null \
  || fail "verification failed: this file does not come from the expected release. Nothing was installed."
say "Verified."

if [ "$OS" = Linux ]; then
  say "Installing the package (sudo)…"
  sudo apt-get install -y "$TMP/$FILE"
  say ""
  say "✅ Sténo $VERSION is installed. Launch it from your applications, then check with: steno doctor"
  exit 0
fi

# macOS
mkdir -p "$TMP/pkg"
tar -xzf "$TMP/$FILE" -C "$TMP/pkg"
[ -d "$TMP/pkg/Steno.app" ] && [ -f "$TMP/pkg/steno" ] || fail "unexpected archive content."

mkdir -p "$APP_DIR" "$BIN_DIR"
NEW="$APP_DIR/.Steno.app.new"
rm -rf "$NEW"
cp -R "$TMP/pkg/Steno.app" "$NEW"
# Verified above: drop the "downloaded from the Internet" flag a browser may have set.
xattr -dr com.apple.quarantine "$NEW" 2>/dev/null || true
# Same signature as `make install`: the stable local identity, otherwise ad hoc.
ID=$(sh "$TMP/pkg/local-signing.sh" 2>/dev/null) || ID=-
if ! codesign --force --deep --sign "$ID" "$NEW" 2>/dev/null; then
  ID=-
  codesign --force --deep --sign - "$NEW"
fi
rm -rf "$APP_DIR/Steno.app"
mv "$NEW" "$APP_DIR/Steno.app"
install -m 755 "$TMP/pkg/steno" "$BIN_DIR/steno"

if [ "$ID" = "-" ]; then
  say "Ad hoc signature (macOS will ask for permissions again after each update)."
else
  say "Signed with $ID: permissions are kept across versions."
fi

say "Downloading the speech models (first install only, a few minutes)…"
"$APP_DIR/Steno.app/Contents/MacOS/steno-audio" --prefetch-models \
  || say "Model download failed: Sténo will retry on first use."

say ""
say "✅ Sténo $VERSION is installed. CLI: $BIN_DIR/steno (add this directory to your PATH if needed)."
say "   Launch Sténo from $APP_DIR, then check with: steno doctor"
