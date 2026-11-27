# Passation — état du projet Sténo et reste à faire

> Document pour la personne (ou l'agent) qui reprend le projet. À lire avec `PLAN.md`
> (le besoin), `docs/contracts.md` (les interfaces), `docs/adapters.md` (les CLI d'agent),
> `docs/spikes.md` (ce qui doit être validé sur Mac).

## 1. Résumé

Tout le logiciel décrit dans `PLAN.md` est **codé**. Tout ce qui ne dépend pas de macOS a été
**exécuté et testé sous Linux** : cœur Rust, CLI, interface, et app Tauri lancée dans un écran
virtuel, avec un sidecar audio simulé et un faux CLI d'agent.

Trois choses n'ont **jamais tourné** :
- le sidecar audio Swift (`audio/StenoAudio`), faute de Mac ;
- les vrais CLI d'agent avec un vrai abonnement ;
- la capture Teams.

C'est l'objet de la phase 0 (`docs/spikes.md`), à faire en premier sur un Mac.

Règles produit ajoutées après le plan initial (appliquées partout) :
0. **Multi-apps** : la détection d'appel couvre une liste d'apps réglable (`audio.call_apps` :
   Teams, Slack, Zoom, Webex, Discord, FaceTime ; navigateur pour Google Meet, désactivé par
   défaut). Le cœur envoie la liste au sidecar (`watch`), le sidecar émet `call` par app, et
   `start.apps` indique l'app dont on capte le son. La réunion garde `call_app`.
1. **L'écoute ne démarre jamais automatiquement.**
   - Un appel Teams détecté donne au plus une notification « Appel Teams détecté — démarrer l'écoute ? ».
   - On démarre l'écoute depuis l'icône de la barre d'état macOS (menu natif), le raccourci ⌃⌥S, ou l'app.
2. **L'écoute ne s'arrête jamais automatiquement.**
   - La fin de l'appel Teams ne l'arrête pas.
   - On l'arrête par le menu, le raccourci, l'app, ou **à la voix** : « Sténo, arrête l'écoute » (intention `stop`).
3. Design : épuré, barre latérale bleu nuit, logo de l'utilisateur (`assets/logo.jpg`, recréé en SVG dans l'UI, icônes générées dans `app/src-tauri/icons/`).

## 2. Organisation du dépôt

| Dossier | Contenu | Taille |
|---|---|---|
| `crates/steno-core` | lib Rust : base SQLite, réglages, file de tâches, session en direct, mot d'appel, anti-écho, adaptateurs d'agent, politique lecture seule, compte rendu, corrélation, diagnostic, répartiteur d'API | ~8 700 lignes |
| `crates/steno-cli` | bin `steno` : ask, doctor, init, export, mcp, meetings, settings, jobs, replay, simulate, serve, daemon, policy-hook | ~1 000 lignes + 580 de tests |
| `app/src-tauri` | app Tauri 2 : barre d'état, fenêtres, raccourcis globaux, notifications, les 49 commandes | ~800 lignes |
| `app/ui` | Svelte 5 + Vite + TS, faux backend, backend HTTP, tests Vitest et Playwright | ~7 500 lignes |
| `audio/StenoAudio` | sidecar Swift : process tap Teams, micro avec annulation d'écho, FluidAudio, diarisation, spikes | ~3 700 lignes + 500 de tests |
| `crates/steno-audio-linux` | sidecar Rust pour Linux, même protocole : PulseAudio / PipeWire, AEC WebRTC, sherpa-onnx (Parakeet, Canary, Silero, pyannote), calendrier EDS / .ics | ~5 000 lignes, tests compris |
| `agent/` | `INSTRUCTIONS.md`, procédures (`playbooks/`), `policy.toml` | |
| `fixtures/` | daily synthétique de 45 min, attendus, faux `claude` | |
| `docs/` | contrats, adaptateurs, spikes, captures, ce document | |

## 3. Architecture en bref

```
Steno.app (Tauri) ──┬─ UI Svelte (WKWebView) ── invoke / événements ──┐
                     └─ steno-core (Rust, même processus) ◄────────────┘
                          │  JSON lines (stdin/stdout)          │ lance claude/codex… (-p, lecture seule)
                          ▼                                     ▼
                     steno-audio (Swift)                  CLI d'agent + crochet « policy-hook »
                     tap Teams + micro → VAD → ASR          (le binaire Sténo lui-même)
CLI `steno` : même cœur ; `steno serve` = même API en HTTP + SSE (tests e2e, dev navigateur)
```

Points clés :
- **Une seule source de vérité pour l'API** : `docs/contracts.md` §5. L'app Tauri
  (`app/src-tauri/src/commands.rs`) et `steno serve` (`steno_core::api::dispatch`) exposent
  exactement les mêmes 49 commandes. Deux tests le vérifient :
  - `api::tests::toutes_les_commandes_du_contrat_sont_connues` ;
  - `tests::commandes_tauri_conformes_au_contrat`.
- **Sécurité de l'agent** (`crates/steno-core/src/policy.rs`, `agents/claude_code.rs`). Avec Claude Code :
  - mode `dontAsk` et liste blanche d'outils ;
  - refus explicites des outils d'écriture ;
  - crochet `PreToolUse` → `<exécutable Sténo> policy-hook <politique.json>`, qui refuse les outils MCP d'écriture (motifs et mots), les commandes shell hors liste, les lectures hors des dépôts cochés, et impose le budget d'appels.

  Les agents sans liste de refus MCP (Codex, Gemini, OpenCode, custom) ont leurs connecteurs grisés et neutralisés.
- **Le cœur est l'unique écrivain** de la base. La CLI écrit seulement dans `jobs`, puis exécute elle-même la tâche si l'app ne tourne pas (battement de cœur `runtime.app_heartbeat`).
- **Cycle d'une réunion** :
  1. `start_meeting` (manuel) → `Start` au sidecar → segments → anti-écho → mot d'appel (canal `me` seulement) → demandes ;
  2. `stop_meeting` (manuel ou « arrête l'écoute ») → tâche `diarize` (si audio) puis `report` ;
  3. puis `correlate` si le mode est `auto` et que la réunion n'est pas exclue ;
  4. l'audio est supprimé selon la rétention.

## 4. Ce qui est fait et vérifié

**Portage Linux (Debian / Ubuntu)** : sidecar `crates/steno-audio-linux` (voir son README pour
les essais réalisés : appel simulé sur PulseAudio, annulation d'écho, débranchement du micro,
redémarrage du serveur son, chaîne complète avec `steno serve`), cœur et app adaptés
(`call_apps` par plateforme, extraction des documents sans `textutil`, diagnostic, actions
`steno-app --toggle/--aside/--note/--show` pour les raccourcis sous Wayland, menu de la barre
d'état complet), `make deps`, `make install` (dans `~/.local`), `make deb`. Reste à valider sur
un vrai bureau : vrais clients Teams for Linux / Slack / Zoom, PipeWire natif, icône
AppIndicator de GNOME, clic sur les notifications.


| Domaine | État | Preuve |
|---|---|---|
| Schéma SQLite, FTS5, migrations, sauvegarde, rétention | ✅ | tests `db::*`, `maintenance` |
| Réglages typés, import/export TOML d'équipe | ✅ | tests `settings::*`, `cli::reglages_d_equipe_sans_donnees_perso` |
| Mot d'appel : détection, mode strict, variantes, mots interdits, contrôles (syllabes, historique, prénoms), aparté, arrêt à la voix | ✅ | tests `wake::*`, acceptation §6.5 (≥ 18/20, 0 faux déclenchement) |
| Anti-écho | ✅ | tests `echo::*`, rejeu de la daily |
| Adaptateur `claude-code` | ✅ sur faux CLI | acceptation §5.6 (sources, reprise de session, refus) |
| Adaptateurs `codex`, `gemini`, `opencode`, `custom` | 🟡 codés, analyse de sortie testée, jamais lancés pour de vrai | tests unitaires des analyseurs |
| Politique lecture seule + crochet | ✅ | tests `policy::*`, `cli::crochet_de_politique`, `doctor --live` sur faux CLI |
| Compte rendu, décisions, actions, intervenants proposés, brouillons | ✅ sur faux CLI | acceptation §7.3 |
| Corrélation (off / à la demande / auto, exclusions, versions) | ✅ sur faux CLI | acceptation §7.3 + `exclusions_et_confidentialite` |
| Écoute manuelle seulement ; fin d'appel Teams sans effet ; arrêt à la voix | ✅ | `appel_teams_propose_sans_demarrer_et_arret_a_la_voix`, `daily_complete…` |
| CLI (ask, export, mcp, settings, policy-hook, serve, simulate) | ✅ | `crates/steno-cli/tests/cli.rs` (binaire en sous-processus) |
| Serveur MCP en lecture seule | ✅ | `cli::serveur_mcp_lecture_seule` |
| Performance de la recherche (500 réunions, 150 000 phrases) | ✅ 16 à 75 ms | `cargo test -p steno-core --release --test perf -- --ignored` |
| App Tauri (commandes, événements, barre d'état, raccourcis) | 🟡 compilée et lancée sous Linux ; menu natif macOS jamais vu | test de conformité des commandes ; essai manuel sous Xvfb |
| Interface (tous les écrans du plan) | ✅ | Vitest + Playwright (maquette et réel), captures `docs/screenshots/` |
| Sidecar Swift | 🔴 jamais compilé | — |

## 5. Tests

État au moment de la passation : **tout est vert**.
- Rust : 74 tests au total :
  - 58 unitaires du cœur ;
  - 3 de l'app ;
  - 1 unitaire de la CLI ;
  - 7 fonctionnels de la CLI ;
  - 5 d'acceptation.
- Vitest : 66 tests.
- Playwright : 16 tests, dont 14 en maquette et 2 scénarios réels.
- Performance : test vert.

| Commande | Contenu |
|---|---|
| `cargo test --workspace` | Rust : unitaires (cœur, app), fonctionnels (CLI en sous-processus), acceptation (pipeline complet avec le faux agent), conformité des commandes |
| `cargo test -p steno-core --release --test perf -- --ignored` | performance de la recherche |
| `cd app/ui && npm run test:unit` | Vitest : formatage, assainissement du markdown, routeur, logique pure |
| `cd app/ui && npm run test:e2e` | Playwright : projet `maquette` (UI + faux backend) et projet `reel` (UI → `steno serve` → cœur → faux `claude` → SQLite) |
| `make test` | tout ce qui précède |
| `swift test` (dans `audio/StenoAudio`) | protocole, correction par vocabulaire, segmenteur, utilitaires (**jamais exécuté**) |

Outils de test :
- `fixtures/daily-45min.jsonl` (régénérable par `fixtures/generate.py`) avec `fixtures/expected.json` ;
- `fixtures/fake-bin/claude`, qui imite `claude -p --output-format stream-json`, `--version` et `mcp list`, et appelle **réellement** le crochet déclaré dans `--settings` ;
- `steno replay <fichier> --speed N`, le simulateur du sidecar : il annonce l'appel Teams puis attend `start`.

Sur Mac, pour les e2e : `cd app/ui && npx playwright install chromium` une fois.

## 6. Reste à faire (par priorité)

### P0 — Phase 0 sur Mac (bloquant, détail dans `docs/spikes.md`)
1. **Compiler le sidecar** (`cd audio/StenoAudio && swift build -c release && swift test`) et corriger les erreurs. Les points incertains sont marqués `À VÉRIFIER` dans le code et listés dans `audio/StenoAudio/README.md` :
   - API FluidAudio 0.18.x : `AsrManager.transcribe(_:decoderState:language:)`, `VadManager`, `OfflineDiarizerManager` ;
   - initialiseurs de `CATapDescription` ;
   - code d'erreur quand l'audio système est refusé.
2. **S4** (Mac pro, MDM) : si l'audio système est bloqué, arrêter et prévenir.
3. **S1** : capture Teams, puis **Slack (huddle) et Zoom**, **lancée depuis Steno.app**, pour vérifier que la permission couvre le sidecar. Plan B : capture en Rust dans le processus principal (`objc2-core-audio`), avec le même protocole.
4. **S2** : qualité de la transcription en français et latence (< 1 s par phrase).
5. **S3** : `steno doctor --live` avec le vrai Claude Code puis Codex :
   - les connecteurs du compte sont-ils disponibles en `-p` ?
   - les options de Codex (`-c mcp_servers={}`, `exec resume`) sont-elles valides ?
6. **S5** : RAM au repos < 120 Mo. Les modèles sont chargés au `start` et libérés au `stop` ; `STENO_PRELOAD_MODELS=1` les précharge.

### P1 — Vérifications de l'app sur macOS
- `make install`, puis l'assistant de premier lancement de bout en bout.
- **Barre d'état** : le clic ouvre le menu natif, les libellés suivent l'état (« ● Écoute en cours — … », « Appel Teams détecté : … »), le « ● » apparaît à côté de l'icône, la case « Réunion confidentielle » fonctionne.
- Raccourcis globaux : ⌃⌥S, ⌃⌥Espace maintenu (aparté, avec événements appui et relâchement), ⌃⌥N.
- Notifications macOS. Le clic sur une notification n'ouvre pas encore la réponse : `tauri-plugin-notification` ne gère pas l'action de clic sur desktop, et le champ `route` est prêt dans `CoreEvent::Notify`.
- Dock : l'icône apparaît quand la fenêtre est ouverte et disparaît à la fermeture (`ActivationPolicy`).
- Signature : `make install` signe avec l'identité locale « Steno Local Signing » (scripts/local-signing.sh) ; l'exigence de signature (identifiant + certificat) est stable, donc les permissions TCC tiennent d'une compilation à l'autre. Signée ad hoc, l'app les perdait à chaque compilation.
- Retour sonore à la détection d'une commande (`afplay`) : ne doit jamais être injecté dans l'audio Teams.

### P2 — Fonctionnel à affiner
- Mot d'appel : régler les verbes et les variantes sur de vraies transcriptions Parakeet, et dérouler le test vocal de l'écran Voix sur Mac.
- Diarisation : correspondance des tours de parole avec les segments `them`, renommage proposé (« Marie ? ») à valider dans l'UI.
- Prompts (`crates/steno-core/src/prompts.rs`, `agent/playbooks/`) : à ajuster sur de vraies réponses (format `TL;DR`, bloc JSON final du compte rendu et de la corrélation).
- `codex`, `gemini`, `opencode` : vérifier les options sur les versions installées, puis mettre à jour `docs/adapters.md`. Si un CLI sait bloquer des outils MCP par motif, passer `tool_deny_list: true` et traduire la politique.
- Pause / reprise de l'écoute : l'état `paused` existe dans le protocole mais n'est pas exposé.
- Clic sur une notification → ouvrir la réponse ; « 5 dernières réponses » directement dans le menu natif.

### Remarques laissées par le travail sur l'UI (non bloquantes)
- `get_settings` renvoie encore `audio.end_grace_s` (obsolète, ignoré) : à retirer lors d'une migration des réglages.
- `steno://navigate` n'est pas émis par `steno serve` (seulement par l'app Tauri). L'UI s'y abonne sans en dépendre.
- En simulation, il n'y a qu'un intervenant « Eux » : pas de diarisation sans audio. La durée vaut 0 min à cause du rejeu accéléré.
- En mode HTTP, `openMain(route)` change la route dans la page et `openExternal` ouvre un onglet.

### Agents, modèles, périphériques : à vérifier sur Mac
- **Antigravity** (`agy`) et **Grok Build** (`grok`) :
  - options écrites d'après la doc officielle, jamais lancées (voir `docs/adapters.md`) ;
  - à vérifier : `agy -p … --output-format json`, la forme réelle de la sortie JSON de `grok`, la syntaxe `--disallowed-tools`, et `--effort` selon le modèle.
- **Claude Code** : vérifier `--effort` sur la version installée.
- **Codex** : vérifier que `-c model_reasoning_effort=…` est accepté par `codex exec`.
- **Périphériques** :
  - micro choisi avec l'annulation d'écho (`kAudioOutputUnitProperty_CurrentDevice` sur `inputNode`) ;
  - changement de micro pendant l'écoute ;
  - AirPods branchés puis débranchés (repli `device_missing`) ;
  - signal joué sur la sortie choisie ;
  - `steno-audio --list-devices`.
  - Liste complète dans `audio/StenoAudio/README.md`, points 16 à 20.

### Design v2 : à vérifier sur Mac
- Translucidité native : la barre latérale à ~60 % d'opacité sur la matière « sidebar », la lisibilité selon le fond d'écran, la fenêtre active ou inactive, et le panneau `#/tray` transparent sur la matière « popover » (`tauri.conf.json` : `transparent`, `windowEffects`, `macOSPrivateApi`).
- Barre latérale façon Arc :
  - position du bouton flottant à côté des boutons de fenêtre (`--tb-left: 78px`, `--tb-top: 2px` dans `Shell.svelte`) ;
  - zone de déplacement de la fenêtre en mode réduit ;
  - ⌘\ sur un clavier AZERTY.
- View Transitions dans le WebKit de macOS 14.2 et plus.
- Test Playwright de la palette ⌘K (`maquette.spec.ts`, navigation ↑↓) : un échec isolé a été observé une fois, puis il est passé plus de 30 fois. Si l'échec revient, chercher ce qui ferme la palette juste après une navigation (perte de focus, transition de vue).

### P3 — Équipe et finitions
- Configuration d'équipe : rédiger un TOML de départ (vocabulaire des services, verbes, exclusions).
- Rendre un vrai Mac disponible en CI (job `macos` en `workflow_dispatch` seulement, car les minutes macOS coûtent plus cher).
- Consentement RGPD : texte d'information des participants (hors code).

## 7. Commandes utiles

```bash
make test                     # tous les tests
make serve                    # UI + vrai cœur dans le navigateur : http://127.0.0.1:4180/?backend=http
make simulate                 # daily d'exemple dans tout le pipeline, sortie texte
make dev                      # app Tauri en développement (sidecar simulé, faux agent)
make install                  # sur Mac : compile, signe, installe Steno.app + CLI + modèles
steno doctor [--live]        # diagnostic
STENO_HOME=/tmp/x steno …   # dossier de données isolé
RUST_LOG=debug                # journaux détaillés (app : ~/Library/Application Support/Steno/logs/)
```

## 8. Pièges connus

- `tauri-build` exige `app/src-tauri/binaries/steno-audio-<triplet>`. Sous Linux,
  `make sidecar-placeholder` crée un remplaçant ; sur Mac, `make sidecar` copie le binaire Swift.
- En debug, l'app charge l'UI depuis Vite (`http://localhost:5173`) : lancer `npm run dev` dans
  `app/ui`, ou utiliser `make dev`.
- Les dates de l'UI tolèrent les valeurs non ISO : les échéances viennent de l'IA (« vendredi »).
- Le crochet de politique est l'exécutable courant (`current_exe()`) : si l'app est déplacée
  pendant une exécution d'agent, la demande échoue proprement (outil refusé).
- Un processus enfant lancé par un test doit avoir stdout/stderr à `null`, sinon le pipe de
  `cargo test` reste ouvert après un échec.
