# Scribe — compilation locale (PLAN §14). Cibles principales :
#   make install    compile tout, signe (ad hoc), installe Scribe.app et la CLI, télécharge les modèles
#   make test       tous les tests : Rust (unitaires, fonctionnels, acceptation, perf), UI, e2e
#   make test-e2e   e2e Playwright (maquette + réels : UI → scribe serve → cœur → faux agent)
#   make serve      interface + vrai cœur dans un navigateur : http://127.0.0.1:4180/?backend=http
#   make dev        app en mode développement (rechargement à chaud de l'UI)
#   make simulate   rejoue la daily d'exemple dans tout le pipeline (faux agent, sans Mac ni abonnement)

SHELL := /bin/bash
UNAME := $(shell uname -s)
ARCH := $(shell uname -m)
TRIPLE := $(shell rustc -vV | sed -n 's/host: //p')
APP_DIR ?= /Applications
BIN_DIR ?= $(HOME)/.local/bin
SIDECAR := app/src-tauri/binaries/scribe-audio-$(TRIPLE)
TAURI := app/ui/node_modules/.bin/tauri

.PHONY: all install check-prereqs ui sidecar sidecar-placeholder cli app models test test-rust test-ui test-e2e serve dev simulate fixtures clean mcp-config

all: app cli

check-prereqs:
	@command -v cargo >/dev/null || { echo "❌ Rust manquant : https://rustup.rs"; exit 1; }
	@command -v npm >/dev/null || { echo "❌ Node 22 manquant (seulement pour compiler l'UI)"; exit 1; }
ifeq ($(UNAME),Darwin)
	@command -v swift >/dev/null || { echo "❌ Xcode manquant (sidecar Swift)"; exit 1; }
	@sw_vers -productVersion | awk -F. '{ if ($$1 < 14 || ($$1 == 14 && $$2 < 2)) { print "❌ macOS 14.2+ requis"; exit 1 } }'
	@[ "$(ARCH)" = "arm64" ] || echo "⚠️  Mac Intel : non testé (le Neural Engine sert à la transcription)"
endif
	@echo "✅ Prérequis présents"

app/ui/node_modules: app/ui/package.json app/ui/package-lock.json
	cd app/ui && npm ci
	@touch app/ui/node_modules

ui: app/ui/node_modules
	cd app/ui && npm run build

# Sidecar audio : Swift sur macOS ; ailleurs, un remplaçant qui signale son absence.
ifeq ($(UNAME),Darwin)
sidecar:
	cd audio/ScribeAudio && swift build -c release
	mkdir -p app/src-tauri/binaries
	cp audio/ScribeAudio/.build/release/scribe-audio $(SIDECAR)
else
sidecar: sidecar-placeholder
endif

sidecar-placeholder:
	@mkdir -p app/src-tauri/binaries
	@if [ ! -f $(SIDECAR) ]; then \
	  printf '#!/bin/sh\necho "{\\"type\\":\\"error\\",\\"code\\":\\"model_missing\\",\\"message\\":\\"sidecar audio non compilé (macOS uniquement)\\",\\"recoverable\\":false}"\nsleep 2\n' > $(SIDECAR); \
	  chmod +x $(SIDECAR); fi

cli:
	cargo build --release -p scribe-cli

app: check-prereqs ui sidecar
	cd app && ui/node_modules/.bin/tauri build $(if $(filter Darwin,$(UNAME)),--bundles app,--no-bundle)

models:
ifeq ($(UNAME),Darwin)
	$(SIDECAR) --prefetch-models
else
	@echo "(modèles audio : macOS uniquement)"
endif

install: app cli models
ifeq ($(UNAME),Darwin)
	mkdir -p $(APP_DIR)
	rm -rf "$(APP_DIR)/Scribe.app"
	cp -R target/release/bundle/macos/Scribe.app "$(APP_DIR)/"
	# Signature ad hoc (pas de compte développeur Apple) ; le sidecar est signé avec l'app.
	codesign --force --deep --sign - "$(APP_DIR)/Scribe.app"
endif
	mkdir -p $(BIN_DIR)
	cp target/release/scribe $(BIN_DIR)/scribe
	@echo ""
	@echo "✅ Installé. CLI : $(BIN_DIR)/scribe (ajoutez ce dossier à votre PATH si besoin)."
	@echo "   Lancez Scribe depuis $(APP_DIR), puis vérifiez avec : scribe doctor"

test: test-rust test-ui test-e2e

# Rust : unitaires, fonctionnels (CLI en sous-processus), acceptation (faux agent), perf.
test-rust: sidecar-placeholder ui
	cargo test --workspace
	cargo test -p scribe-core --release --test perf -- --ignored

test-ui: app/ui/node_modules
	cd app/ui && npm run check && npm run test:unit && npm run build

# e2e Playwright : maquette + réels (UI → scribe serve → cœur → faux agent → SQLite).
# Sur un Mac neuf : `cd app/ui && npx playwright install chromium` une fois.
test-e2e: app/ui/node_modules
	cargo build -p scribe-cli
	cd app/ui && npm run build && npm run test:e2e

# Interface + vrai cœur dans un navigateur (sidecar simulé, faux agent) :
# ouvrir http://127.0.0.1:4180/?backend=http
serve: ui
	cargo build -p scribe-cli
	PATH="$(CURDIR)/fixtures/fake-bin:$(CURDIR)/target/debug:$$PATH" \
	  SCRIBE_HOME="$${SCRIBE_HOME:-$(CURDIR)/.dev-home}" \
	  SCRIBE_SIDECAR="$(CURDIR)/target/debug/scribe replay $(CURDIR)/fixtures/daily-45min.jsonl --speed 20" \
	  target/debug/scribe serve --port 4180 --ui app/ui/dist

# Développement : UI servie par Vite, sidecar simulé par la daily d'exemple, faux agent.
dev: app/ui/node_modules sidecar-placeholder
	cargo build -p scribe-cli
	cd app && PATH="$(CURDIR)/fixtures/fake-bin:$(CURDIR)/target/debug:$$PATH" \
	  SCRIBE_HOME="$${SCRIBE_HOME:-$(CURDIR)/.dev-home}" \
	  SCRIBE_SIDECAR="$(CURDIR)/target/debug/scribe replay $(CURDIR)/fixtures/daily-45min.jsonl --speed 20" \
	  ui/node_modules/.bin/tauri dev

simulate:
	cargo build -p scribe-cli
	PATH="$(CURDIR)/fixtures/fake-bin:$$PATH" SCRIBE_HOME="$${SCRIBE_HOME:-$(CURDIR)/.dev-home}" \
	  target/debug/scribe simulate fixtures/daily-45min.jsonl

fixtures:
	python3 fixtures/generate.py

mcp-config:
	@echo "Pour donner accès aux réunions à un agent (lecture seule) :"
	@echo "  claude mcp add scribe -- $(BIN_DIR)/scribe mcp"
	@echo "  codex mcp add scribe -- $(BIN_DIR)/scribe mcp"

clean:
	cargo clean
	rm -rf app/ui/dist app/ui/node_modules audio/ScribeAudio/.build .dev-home
