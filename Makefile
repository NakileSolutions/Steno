# Sténo — local build (PLAN §14). Main targets:
#   make install    builds everything and installs the app, the CLI and the audio models
#                   (macOS: signed Steno.app; Linux: ~/.local, desktop entry, no sudo)
#   make deps       Linux: installs the build packages (apt, asks for sudo)
#   make deb        Linux: Debian/Ubuntu package (target/release/bundle/deb/)
#   make test       all tests: Rust (unit, functional, acceptance, perf), UI, e2e
#   make test-e2e   Playwright e2e (mock + real: UI → steno serve → core → fake agent)
#   make serve      UI + real core in a browser: http://127.0.0.1:4180/?backend=http
#   make dev        app in development mode (UI hot reload)
#   make simulate   replays the sample daily through the whole pipeline (fake agent, no Mac or subscription)

SHELL := /bin/bash
UNAME := $(shell uname -s)
ARCH := $(shell uname -m)
TRIPLE := $(shell rustc -vV | sed -n 's/host: //p')
APP_DIR ?= /Applications
BIN_DIR ?= $(HOME)/.local/bin
# Linux: the app and its sidecar go in $(LIB_DIR), the desktop entry and icons in $(DATA_DIR).
LIB_DIR ?= $(HOME)/.local/lib/steno
DATA_DIR ?= $(HOME)/.local/share
SIDECAR := app/src-tauri/binaries/steno-audio-$(TRIPLE)
TAURI := app/ui/node_modules/.bin/tauri

# Linux sidecar: the sherpa-onnx library (ONNX Runtime included) is downloaded beforehand with
# curl or wget, which follow the system network settings (proxy, certificate authorities).
SHERPA_VERSION := $(shell sed -n '/^name = "sherpa-onnx-sys"/{n;s/version = "\(.*\)"/\1/p;}' Cargo.lock)
SHERPA_ARCH := $(if $(filter aarch64 arm64,$(ARCH)),aarch64,x64)
SHERPA_ARCHIVE_DIR := $(CURDIR)/target/sherpa-onnx
SHERPA_ARCHIVE := $(SHERPA_ARCHIVE_DIR)/sherpa-onnx-v$(SHERPA_VERSION)-linux-$(SHERPA_ARCH)-static-lib.tar.bz2

# Debian / Ubuntu build packages (Tauri, PulseAudio, WebRTC echo cancellation).
APT_PACKAGES := build-essential pkg-config curl git patch clang libclang-dev meson ninja-build \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev libpulse-dev
PKG_CONFIG_LIBS := webkit2gtk-4.1 gtk+-3.0 ayatana-appindicator3-0.1 librsvg-2.0 libpulse

.PHONY: deps deb test-swift all install check-prereqs ui sidecar sidecar-placeholder cli app models test test-rust test-ui test-e2e serve dev simulate fixtures clean mcp-config

all: app cli

check-prereqs:
	@command -v cargo >/dev/null || { echo "❌ Rust missing: https://rustup.rs"; exit 1; }
	@command -v npm >/dev/null || { echo "❌ Node 22 missing (only needed to build the UI)"; exit 1; }
ifeq ($(UNAME),Darwin)
	@command -v swift >/dev/null || { echo "❌ Xcode missing (Swift sidecar)"; exit 1; }
	@sw_vers -productVersion | awk -F. '{ if ($$1 < 14 || ($$1 == 14 && $$2 < 2)) { print "❌ macOS 14.2+ required"; exit 1 } }'
	@[ "$(ARCH)" = "arm64" ] || echo "⚠️  Intel Mac: untested (transcription uses the Neural Engine)"
else
	@missing=""; \
	  for c in pkg-config cc c++ meson ninja patch; do command -v $$c >/dev/null || missing="$$missing $$c"; done; \
	  command -v curl >/dev/null || command -v wget >/dev/null || missing="$$missing curl"; \
	  for l in $(PKG_CONFIG_LIBS); do pkg-config --exists $$l 2>/dev/null || missing="$$missing $$l"; done; \
	  if [ -n "$$missing" ]; then \
	    echo "❌ Missing:$$missing"; \
	    echo "   Install the build packages: make deps"; \
	    echo "   (sudo apt install $(APT_PACKAGES))"; exit 1; fi
	@{ [ -S "$$XDG_RUNTIME_DIR/pulse/native" ] || command -v pactl >/dev/null; } \
	  || echo "⚠️  No sound server found: Sténo needs PipeWire (with pipewire-pulse) or PulseAudio."
endif
	@echo "✅ Prerequisites present"

# Linux (Debian / Ubuntu): packages needed to build.
deps:
ifeq ($(UNAME),Darwin)
	@echo "macOS: install Xcode (or the Command Line Tools), Rust (rustup) and Node 22."
else
	sudo apt-get update
	sudo apt-get install -y $(APT_PACKAGES)
	@command -v cargo >/dev/null || echo "Also install Rust: https://rustup.rs"
	@command -v npm >/dev/null || echo "Also install Node 22: https://nodejs.org (or nvm)"
endif

app/ui/node_modules: app/ui/package.json app/ui/package-lock.json
	cd app/ui && npm ci
	@touch app/ui/node_modules

ui: app/ui/node_modules
	cd app/ui && npm run build

# Audio sidecar: Swift on macOS, Rust (crates/steno-audio-linux) on Linux.
ifeq ($(UNAME),Darwin)
sidecar:
	cd audio/StenoAudio && swift build -c release
	mkdir -p app/src-tauri/binaries
	cp audio/StenoAudio/.build/release/steno-audio $(SIDECAR)
else ifeq ($(UNAME),Linux)
$(SHERPA_ARCHIVE):
	@mkdir -p $(SHERPA_ARCHIVE_DIR)
	@echo "Downloading sherpa-onnx $(SHERPA_VERSION) (transcription library)…"
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
	  printf '#!/bin/sh\necho "{\\"type\\":\\"error\\",\\"code\\":\\"model_missing\\",\\"message\\":\\"audio sidecar not built (make sidecar)\\",\\"recoverable\\":false}"\nsleep 2\n' > $(SIDECAR); \
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
	# Sign with the local "Steno Local Signing" identity (created once in the keychain,
	# see scripts/local-signing.sh): the app keeps the same identity across versions and
	# macOS keeps its permissions. Otherwise, ad hoc signing. The sidecar is signed with the app.
	@ID=$$(scripts/local-signing.sh 2>/dev/null) || ID=-; \
	  codesign --force --deep --sign "$$ID" "$(APP_DIR)/Steno.app" 2>/dev/null \
	    || { ID=-; codesign --force --deep --sign - "$(APP_DIR)/Steno.app"; }; \
	  if [ "$$ID" = "-" ]; then echo "Ad hoc signature (macOS will ask for permissions again after each update)."; \
	  else echo "Signed with $$ID: permissions are kept across versions."; fi
else
	# Linux: install into the user's home directory (no sudo). The app is launched from the
	# "Sténo" desktop entry; the sidecar sits next to it (the app looks for it there).
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
	@echo "✅ Installed. CLI: $(BIN_DIR)/steno (add this directory to your PATH if needed)."
ifeq ($(UNAME),Darwin)
	@echo "   Launch Sténo from $(APP_DIR), then check with: steno doctor"
else
	@echo "   Launch Sténo from your applications (or $(BIN_DIR)/steno-app), then: steno doctor"
endif

# Linux: Debian / Ubuntu package (app, sidecar, CLI, desktop entry). The audio models are
# downloaded on first launch (or with `steno-audio --prefetch-models`).
deb: check-prereqs ui sidecar cli
	cd app && ui/node_modules/.bin/tauri build --bundles deb
	@ls -1 target/release/bundle/deb/*.deb

test: test-rust test-swift test-ui test-e2e

# Swift: audio sidecar (protocol, filters, retranscription), macOS only.
test-swift:
ifeq ($(UNAME),Darwin)
	cd audio/StenoAudio && swift test
else
	@echo "(Swift tests: macOS only)"
endif

# Rust: unit, functional (CLI as a subprocess), acceptance (fake agent), perf.
test-rust: sidecar-placeholder ui $(if $(filter Linux,$(UNAME)),$(SHERPA_ARCHIVE))
	SHERPA_ONNX_ARCHIVE_DIR=$(SHERPA_ARCHIVE_DIR) cargo test --workspace
	cargo test -p steno-core --release --test perf -- --ignored

test-ui: app/ui/node_modules
	cd app/ui && npm run check && npm run test:unit && npm run build

# Playwright e2e: mock + real (UI → steno serve → core → fake agent → SQLite).
# On a fresh Mac: run `cd app/ui && npx playwright install chromium` once.
test-e2e: app/ui/node_modules
	cargo build -p steno-cli
	cd app/ui && npm run build && npm run test:e2e

# UI + real core in a browser (simulated sidecar, fake agent):
# open http://127.0.0.1:4180/?backend=http
serve: ui
	cargo build -p steno-cli
	PATH="$(CURDIR)/fixtures/fake-bin:$(CURDIR)/target/debug:$$PATH" \
	  STENO_HOME="$${STENO_HOME:-$(CURDIR)/.dev-home}" \
	  STENO_SIDECAR="$(CURDIR)/target/debug/steno replay $(CURDIR)/fixtures/daily-45min.jsonl --speed 20" \
	  target/debug/steno serve --port 4180 --ui app/ui/dist

# Development: UI served by Vite, sidecar simulated with the sample daily, fake agent.
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
	@echo "To give an agent access to your meetings (read-only):"
	@echo "  claude mcp add steno -- $(BIN_DIR)/steno mcp"
	@echo "  codex mcp add steno -- $(BIN_DIR)/steno mcp"

clean:
	cargo clean
	rm -rf app/ui/dist app/ui/node_modules audio/StenoAudio/.build .dev-home
