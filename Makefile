# Sténo — compilation locale (PLAN §14). Cibles principales :
#   make install    compile tout, signe (ad hoc), installe Steno.app et la CLI, télécharge les modèles
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
SIDECAR := app/src-tauri/binaries/steno-audio-$(TRIPLE)
TAURI := app/ui/node_modules/.bin/tauri

.PHONY: test-swift all install check-prereqs ui sidecar sidecar-placeholder cli app models test test-rust test-ui test-e2e serve dev simulate fixtures clean mcp-config

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
	cd audio/StenoAudio && swift build -c release
	mkdir -p app/src-tauri/binaries
	cp audio/StenoAudio/.build/release/steno-audio $(SIDECAR)
else
sidecar: sidecar-placeholder
endif

sidecar-placeholder:
	@mkdir -p app/src-tauri/binaries
	@if [ ! -f $(SIDECAR) ]; then \
	  printf '#!/bin/sh\necho "{\\"type\\":\\"error\\",\\"code\\":\\"model_missing\\",\\"message\\":\\"sidecar audio non compilé (macOS uniquement)\\",\\"recoverable\\":false}"\nsleep 2\n' > $(SIDECAR); \
	  chmod +x $(SIDECAR); fi

cli:
	cargo build --release -p steno-cli

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
endif
	mkdir -p $(BIN_DIR)
	cp target/release/steno $(BIN_DIR)/steno
	@echo ""
	@echo "✅ Installé. CLI : $(BIN_DIR)/steno (ajoutez ce dossier à votre PATH si besoin)."
	@echo "   Lancez Sténo depuis $(APP_DIR), puis vérifiez avec : steno doctor"

test: test-rust test-swift test-ui test-e2e

# Swift : sidecar audio (protocole, filtres, retranscription), macOS uniquement.
test-swift:
ifeq ($(UNAME),Darwin)
	cd audio/StenoAudio && swift test
else
	@echo "(tests Swift : macOS uniquement)"
endif

# Rust : unitaires, fonctionnels (CLI en sous-processus), acceptation (faux agent), perf.
test-rust: sidecar-placeholder ui
	cargo test --workspace
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
