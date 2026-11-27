# Sténo — compilation locale (PLAN §14). Cibles principales :
#   make install    compile tout et installe l'app, la CLI et les modèles audio
#                   (macOS : Steno.app signée ; Linux : ~/.local, raccourci de bureau, sans sudo)
#   make deps       Linux : installe les paquets de compilation (apt, demande sudo)
#   make deb        Linux : paquet Debian/Ubuntu (target/release/bundle/deb/)
#   make test       tous les tests : Rust (unitaires, fonctionnels, acceptation, perf), UI, e2e
#   make test-e2e   e2e Playwright (maquette + réels : UI → steno serve → cœur → faux agent)
#   make serve      interface + vrai cœur dans un navigateur : http://127.0.0.1:4180/?backend=http
#   make dev        app en mode développement (rechargement à chaud de l'UI)
#   make simulate   rejoue la daily d'exemple dans tout le pipeline (faux agent, sans Mac ni abonnement)

SHELL := /bin/bash
UNAME := $(shell uname -s)
ARCH := $(shell uname -m)
TRIPLE := $(shell rustc -vV | sed -n 's/host: //p')
APP_DIR ?= /Applications
BIN_DIR ?= $(HOME)/.local/bin
# Linux : l'app et son sidecar vont dans $(LIB_DIR), le raccourci de bureau et les icônes dans $(DATA_DIR).
LIB_DIR ?= $(HOME)/.local/lib/steno
DATA_DIR ?= $(HOME)/.local/share
SIDECAR := app/src-tauri/binaries/steno-audio-$(TRIPLE)
TAURI := app/ui/node_modules/.bin/tauri

# Sidecar Linux : la bibliothèque sherpa-onnx (ONNX Runtime compris) est téléchargée d'avance par
# curl ou wget, qui suivent la configuration réseau du système (proxy, autorités de certification).
SHERPA_VERSION := $(shell sed -n '/^name = "sherpa-onnx-sys"/{n;s/version = "\(.*\)"/\1/p;}' Cargo.lock)
SHERPA_ARCH := $(if $(filter aarch64 arm64,$(ARCH)),aarch64,x64)
SHERPA_ARCHIVE_DIR := $(CURDIR)/target/sherpa-onnx
SHERPA_ARCHIVE := $(SHERPA_ARCHIVE_DIR)/sherpa-onnx-v$(SHERPA_VERSION)-linux-$(SHERPA_ARCH)-static-lib.tar.bz2

# Paquets de compilation Debian / Ubuntu (Tauri, PulseAudio, annulation d'écho WebRTC).
APT_PACKAGES := build-essential pkg-config curl git patch clang libclang-dev meson ninja-build \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev libpulse-dev
PKG_CONFIG_LIBS := webkit2gtk-4.1 gtk+-3.0 ayatana-appindicator3-0.1 librsvg-2.0 libpulse

.PHONY: deps deb test-swift all install check-prereqs ui sidecar sidecar-placeholder cli app models test test-rust test-ui test-e2e serve dev simulate fixtures clean mcp-config

all: app cli

check-prereqs:
	@command -v cargo >/dev/null || { echo "❌ Rust manquant : https://rustup.rs"; exit 1; }
	@command -v npm >/dev/null || { echo "❌ Node 22 manquant (seulement pour compiler l'UI)"; exit 1; }
ifeq ($(UNAME),Darwin)
	@command -v swift >/dev/null || { echo "❌ Xcode manquant (sidecar Swift)"; exit 1; }
	@sw_vers -productVersion | awk -F. '{ if ($$1 < 14 || ($$1 == 14 && $$2 < 2)) { print "❌ macOS 14.2+ requis"; exit 1 } }'
	@[ "$(ARCH)" = "arm64" ] || echo "⚠️  Mac Intel : non testé (le Neural Engine sert à la transcription)"
else
	@missing=""; \
	  for c in pkg-config cc c++ meson ninja patch; do command -v $$c >/dev/null || missing="$$missing $$c"; done; \
	  command -v curl >/dev/null || command -v wget >/dev/null || missing="$$missing curl"; \
	  for l in $(PKG_CONFIG_LIBS); do pkg-config --exists $$l 2>/dev/null || missing="$$missing $$l"; done; \
	  if [ -n "$$missing" ]; then \
	    echo "❌ Manquant :$$missing"; \
	    echo "   Installez les paquets de compilation : make deps"; \
	    echo "   (sudo apt install $(APT_PACKAGES))"; exit 1; fi
	@{ [ -S "$$XDG_RUNTIME_DIR/pulse/native" ] || command -v pactl >/dev/null; } \
	  || echo "⚠️  Serveur son introuvable : Sténo a besoin de PipeWire (avec pipewire-pulse) ou de PulseAudio."
endif
	@echo "✅ Prérequis présents"

# Linux (Debian / Ubuntu) : paquets nécessaires à la compilation.
deps:
ifeq ($(UNAME),Darwin)
	@echo "macOS : installez Xcode (ou les Command Line Tools), Rust (rustup) et Node 22."
else
	sudo apt-get update
	sudo apt-get install -y $(APT_PACKAGES)
	@command -v cargo >/dev/null || echo "Installez aussi Rust : https://rustup.rs"
	@command -v npm >/dev/null || echo "Installez aussi Node 22 : https://nodejs.org (ou nvm)"
endif

app/ui/node_modules: app/ui/package.json app/ui/package-lock.json
	cd app/ui && npm ci
	@touch app/ui/node_modules

ui: app/ui/node_modules
	cd app/ui && npm run build

# Sidecar audio : Swift sur macOS, Rust (crates/steno-audio-linux) sous Linux.
ifeq ($(UNAME),Darwin)
sidecar:
	cd audio/StenoAudio && swift build -c release
	mkdir -p app/src-tauri/binaries
	cp audio/StenoAudio/.build/release/steno-audio $(SIDECAR)
else ifeq ($(UNAME),Linux)
$(SHERPA_ARCHIVE):
	@mkdir -p $(SHERPA_ARCHIVE_DIR)
	@echo "Téléchargement de sherpa-onnx $(SHERPA_VERSION) (bibliothèque de transcription)…"
	@url=https://github.com/k2-fsa/sherpa-onnx/releases/download/v$(SHERPA_VERSION)/$(notdir $@); \
	  if command -v curl >/dev/null; then curl -fL --retry 3 -o $@.part "$$url"; else wget -O $@.part "$$url"; fi && mv $@.part $@

sidecar: $(SHERPA_ARCHIVE)
	SHERPA_ONNX_ARCHIVE_DIR=$(SHERPA_ARCHIVE_DIR) cargo build --release -p steno-audio-linux
	mkdir -p app/src-tauri/binaries
	cp target/release/steno-audio $(SIDECAR)
else
sidecar: sidecar-placeholder
endif

sidecar-placeholder:
	@mkdir -p app/src-tauri/binaries
	@if [ ! -f $(SIDECAR) ]; then \
	  printf '#!/bin/sh\necho "{\\"type\\":\\"error\\",\\"code\\":\\"model_missing\\",\\"message\\":\\"sidecar audio non compilé (make sidecar)\\",\\"recoverable\\":false}"\nsleep 2\n' > $(SIDECAR); \
	  chmod +x $(SIDECAR); fi

cli:
	cargo build --release -p steno-cli

app: check-prereqs ui sidecar
	cd app && ui/node_modules/.bin/tauri build $(if $(filter Darwin,$(UNAME)),--bundles app,--no-bundle)

models:
	$(SIDECAR) --prefetch-models

install: app cli models
ifeq ($(UNAME),Darwin)
	mkdir -p $(APP_DIR)
	rm -rf "$(APP_DIR)/Steno.app"
	cp -R target/release/bundle/macos/Steno.app "$(APP_DIR)/"
	# Signature avec l'identité locale « Steno Local Signing » (créée une fois dans le trousseau,
	# voir scripts/local-signing.sh) : l'app garde la même identité d'une version à l'autre et
	# macOS conserve ses autorisations. À défaut, signature ad hoc. Le sidecar est signé avec l'app.
	@ID=$$(scripts/local-signing.sh 2>/dev/null) || ID=-; \
	  codesign --force --deep --sign "$$ID" "$(APP_DIR)/Steno.app" 2>/dev/null \
	    || { ID=-; codesign --force --deep --sign - "$(APP_DIR)/Steno.app"; }; \
	  if [ "$$ID" = "-" ]; then echo "Signature ad hoc (macOS redemandera les autorisations après chaque mise à jour)."; \
	  else echo "Signé avec « $$ID » : les autorisations sont conservées d'une version à l'autre."; fi
else
	# Linux : installation dans le dossier de l'utilisateur (sans sudo). L'app est lancée par le
	# raccourci de bureau « Sténo » ; le sidecar est à côté d'elle (l'app l'y cherche).
	mkdir -p $(LIB_DIR) $(BIN_DIR) $(DATA_DIR)/applications
	install -m 755 target/release/steno-app $(LIB_DIR)/steno-app
	install -m 755 $(SIDECAR) $(LIB_DIR)/steno-audio
	ln -sf $(LIB_DIR)/steno-app $(BIN_DIR)/steno-app
	for s in 32 128 256; do mkdir -p $(DATA_DIR)/icons/hicolor/$${s}x$${s}/apps; done
	install -m 644 app/src-tauri/icons/32x32.png $(DATA_DIR)/icons/hicolor/32x32/apps/steno.png
	install -m 644 app/src-tauri/icons/128x128.png $(DATA_DIR)/icons/hicolor/128x128/apps/steno.png
	install -m 644 app/src-tauri/icons/128x128@2x.png $(DATA_DIR)/icons/hicolor/256x256/apps/steno.png
	sed -e 's|@EXEC@|$(LIB_DIR)/steno-app|g' packaging/linux/steno.desktop > $(DATA_DIR)/applications/steno.desktop
	-command -v update-desktop-database >/dev/null && update-desktop-database -q $(DATA_DIR)/applications
	-command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t $(DATA_DIR)/icons/hicolor
endif
	mkdir -p $(BIN_DIR)
	cp target/release/steno $(BIN_DIR)/steno
	@echo ""
	@echo "✅ Installé. CLI : $(BIN_DIR)/steno (ajoutez ce dossier à votre PATH si besoin)."
ifeq ($(UNAME),Darwin)
	@echo "   Lancez Sténo depuis $(APP_DIR), puis vérifiez avec : steno doctor"
else
	@echo "   Lancez « Sténo » depuis les applications (ou $(BIN_DIR)/steno-app), puis : steno doctor"
endif

# Linux : paquet Debian / Ubuntu (app, sidecar, CLI, raccourci de bureau). Les modèles audio sont
# téléchargés au premier lancement (ou par `steno-audio --prefetch-models`).
deb: check-prereqs ui sidecar cli
	cd app && ui/node_modules/.bin/tauri build --bundles deb
	@ls -1 target/release/bundle/deb/*.deb

test: test-rust test-swift test-ui test-e2e

# Swift : sidecar audio (protocole, filtres, retranscription), macOS uniquement.
test-swift:
ifeq ($(UNAME),Darwin)
	cd audio/StenoAudio && swift test
else
	@echo "(tests Swift : macOS uniquement)"
endif

# Rust : unitaires, fonctionnels (CLI en sous-processus), acceptation (faux agent), perf.
test-rust: sidecar-placeholder ui $(if $(filter Linux,$(UNAME)),$(SHERPA_ARCHIVE))
	SHERPA_ONNX_ARCHIVE_DIR=$(SHERPA_ARCHIVE_DIR) cargo test --workspace
	cargo test -p steno-core --release --test perf -- --ignored

test-ui: app/ui/node_modules
	cd app/ui && npm run check && npm run test:unit && npm run build

# e2e Playwright : maquette + réels (UI → steno serve → cœur → faux agent → SQLite).
# Sur un Mac neuf : `cd app/ui && npx playwright install chromium` une fois.
test-e2e: app/ui/node_modules
	cargo build -p steno-cli
	cd app/ui && npm run build && npm run test:e2e

# Interface + vrai cœur dans un navigateur (sidecar simulé, faux agent) :
# ouvrir http://127.0.0.1:4180/?backend=http
serve: ui
	cargo build -p steno-cli
	PATH="$(CURDIR)/fixtures/fake-bin:$(CURDIR)/target/debug:$$PATH" \
	  STENO_HOME="$${STENO_HOME:-$(CURDIR)/.dev-home}" \
	  STENO_SIDECAR="$(CURDIR)/target/debug/steno replay $(CURDIR)/fixtures/daily-45min.jsonl --speed 20" \
	  target/debug/steno serve --port 4180 --ui app/ui/dist

# Développement : UI servie par Vite, sidecar simulé par la daily d'exemple, faux agent.
dev: app/ui/node_modules sidecar-placeholder
	cargo build -p steno-cli
	cd app && PATH="$(CURDIR)/fixtures/fake-bin:$(CURDIR)/target/debug:$$PATH" \
	  STENO_HOME="$${STENO_HOME:-$(CURDIR)/.dev-home}" \
	  STENO_SIDECAR="$(CURDIR)/target/debug/steno replay $(CURDIR)/fixtures/daily-45min.jsonl --speed 20" \
	  ui/node_modules/.bin/tauri dev

simulate:
	cargo build -p steno-cli
	PATH="$(CURDIR)/fixtures/fake-bin:$$PATH" STENO_HOME="$${STENO_HOME:-$(CURDIR)/.dev-home}" \
	  target/debug/steno simulate fixtures/daily-45min.jsonl

fixtures:
	python3 fixtures/generate.py

mcp-config:
	@echo "Pour donner accès aux réunions à un agent (lecture seule) :"
	@echo "  claude mcp add steno -- $(BIN_DIR)/steno mcp"
	@echo "  codex mcp add steno -- $(BIN_DIR)/steno mcp"

clean:
	cargo clean
	rm -rf app/ui/dist app/ui/node_modules audio/StenoAudio/.build .dev-home
