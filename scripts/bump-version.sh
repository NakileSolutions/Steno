#!/bin/sh
# Change le numéro de version partout (app, CLI, cœur, interface, sidecar).
# Usage : scripts/bump-version.sh 0.2.0
set -eu
NEW="${1:?usage : scripts/bump-version.sh X.Y.Z}"
echo "$NEW" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || { echo "version invalide : $NEW (attendu X.Y.Z)"; exit 1; }
cd "$(dirname "$0")/.."
OLD=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
sed -i '' "s/^version = \"$OLD\"/version = \"$NEW\"/" Cargo.toml
sed -i '' "s/\"version\": \"$OLD\"/\"version\": \"$NEW\"/" app/src-tauri/tauri.conf.json app/ui/package.json
sed -i '' "s/static let version = \"$OLD\"/static let version = \"$NEW\"/" audio/StenoAudio/Sources/StenoAudioKit/Protocol.swift
sed -i '' "s#<string>$OLD</string>#<string>$NEW</string>#" audio/StenoAudio/Info.plist
(cd app/ui && npm install --package-lock-only --silent >/dev/null)
cargo update --workspace --quiet 2>/dev/null || cargo generate-lockfile --quiet
echo "Version $OLD → $NEW. Vérifiez puis commitez : git commit -am \"Version $NEW\""
