#!/bin/sh
# Changes the version number everywhere (app, CLI, core, UI, sidecar).
# Usage: scripts/bump-version.sh 0.2.0
set -eu
NEW="${1:?usage: scripts/bump-version.sh X.Y.Z}"
echo "$NEW" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || { echo "invalid version: $NEW (expected X.Y.Z)"; exit 1; }
cd "$(dirname "$0")/.."
OLD=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
# In-place edit that works with both BSD sed (macOS) and GNU sed (Linux).
replace() { expr="$1"; shift; for f in "$@"; do sed "$expr" "$f" > "$f.tmp" && cat "$f.tmp" > "$f" && rm "$f.tmp"; done; }
replace "s/^version = \"$OLD\"/version = \"$NEW\"/" Cargo.toml
replace "s/\"version\": \"$OLD\"/\"version\": \"$NEW\"/" app/src-tauri/tauri.conf.json app/ui/package.json
replace "s/static let version = \"$OLD\"/static let version = \"$NEW\"/" audio/StenoAudio/Sources/StenoAudioKit/Protocol.swift
replace "s#<string>$OLD</string>#<string>$NEW</string>#" audio/StenoAudio/Info.plist
(cd app/ui && npm install --package-lock-only --silent >/dev/null)
cargo update --workspace --quiet 2>/dev/null || cargo generate-lockfile --quiet
echo "Version $OLD → $NEW. Review, then commit: git commit -am \"Version $NEW\""
