# Phase 0 — spikes

> **État** : le code a été écrit dans un environnement **Linux** (sans Mac, sans Teams, sans
> abonnement d'agent). Tout ce qui ne dépend pas de macOS a été vérifié ici (voir « Fait » plus
> bas). Les spikes S1 à S5 **restent à exécuter sur un Mac** : ils sont bloquants avant un usage
> réel. Les procédures détaillées du sidecar sont dans `audio/ScribeAudio/README.md`.

## Résumé

| Spike | État | Ce qui a été fait ici | Ce qu'il reste à faire sur le Mac |
|---|---|---|---|
| S1 Capture | ⏳ à faire | Sidecar Swift écrit (process tap sur tout l'arbre Teams, AVAudioEngine + voice processing, `--spike-record`) ; **non compilé** (pas de toolchain Swift ici) | Compiler, enregistrer `me.wav` + `them.wav` pendant un vrai appel, **lancé depuis Scribe.app** |
| S2 ASR | ⏳ à faire | Intégration FluidAudio (Parakeet TDT v3, Silero VAD) derrière un protocole ; `--spike-transcribe` | Mesurer la qualité en français et la latence (< 1 s par phrase) ; comparer WhisperKit si besoin |
| S3 Agents | 🟡 en partie | Options de `claude` 2.1.332 vérifiées (`--help`) ; politique lecture seule testée de bout en bout avec un faux `claude` qui appelle réellement le crochet | `scribe doctor --live` avec le vrai Claude Code puis Codex : MCP visibles en `-p`, écriture refusée |
| S4 Poste de travail | ⏳ à faire | `scribe doctor` affiche l'état des permissions remontées par le sidecar et la marche à suivre | Accorder micro, audio système, accessibilité sur le Mac pro ; si le MDM bloque : **arrêter et prévenir** |
| S5 Empreinte | ⏳ à faire | App Tauri compilée et lancée sous Linux (Xvfb) avec le vrai cœur | Mesurer la RAM au repos de Scribe.app (objectif < 120 Mo) |

## S1 — Capture (bloquant)

But : deux pistes audibles (`me.wav`, `them.wav`) pendant un vrai appel Teams, **avec le sidecar
lancé par l'app** (la permission « Enregistrement audio du système » accordée à Scribe.app
doit couvrir le sidecar embarqué).

```bash
cd audio/ScribeAudio && swift build -c release
.build/release/scribe-audio --list-audio-processes        # repérer les processus audio de Teams
.build/release/scribe-audio --spike-record 60 ~/Desktop/s1 # en direct depuis le Terminal
make install && open ~/Applications/Scribe.app             # puis démarrer une réunion depuis l'app
```

Critères : pistes audibles et séparées ; débrancher / rebrancher le casque → reprise < 5 s.
Si la permission ne couvre pas le sidecar : plan B = capture en Rust (`objc2-core-audio`) dans
le processus principal (le protocole JSON lines reste le même, le transport devient interne).

## S2 — Transcription

```bash
.build/release/scribe-audio --prefetch-models
.build/release/scribe-audio --spike-transcribe ~/Desktop/s1/them.wav
```

Critères : français lisible, noms techniques (ms-booking, APP-1234) compris ou corrigés par le
vocabulaire, < 1 s par phrase sur Apple Silicon. Points à vérifier dans le code :
`audio/ScribeAudio/README.md` § « Points À VÉRIFIER » (API FluidAudio 0.18.x notamment).

## S3 — Agents

```bash
scribe doctor --live
```

1. Claude Code : `claude mcp list`, puis vérifier dans Scribe (Réglages → Connecteurs →
   Rafraîchir) que les connecteurs du compte apparaissent et qu'une demande en direct peut les
   utiliser en mode `-p`. Plan B si absents : `claude mcp add --transport http …`.
2. Tenter un outil d'écriture : `doctor --live` demande à l'agent de poster sur Slack et de
   créer un ticket ; aucun outil d'écriture ne doit apparaître dans `tools_used`.
3. Codex : vérifier `codex exec --json`, `codex exec resume`, `-c mcp_servers={}`
   (voir `docs/adapters.md`).

Vérifié ici (`cargo test -p scribe-cli --test acceptance`) avec `fixtures/fake-bin/claude`, qui
imite `claude -p --output-format stream-json` et appelle le crochet déclaré dans `--settings` :
- l'appel `mcp__slack__slack_send_message` est refusé par le crochet même si le connecteur
  Slack est coché ; `git push` est refusé ; `rg` passe ;
- une relance dans la même réunion reprend la session (`--resume <id>`) ;
- « Poste ça sur Slack » est refusé localement, sans appel à l'agent.

## S4 — Poste de travail

Sur le Mac professionnel : lancer Scribe.app, accepter les trois demandes, puis `scribe doctor`.
Si « Audio du système » est refusé ou absent des Réglages Système (profil MDM) :
**arrêter le projet sur ce poste et prévenir** (pas de contournement).

## S5 — Empreinte

```bash
make install && open ~/Applications/Scribe.app
# fenêtre fermée, pas de réunion, après 1 min :
ps -o rss=,comm= -p $(pgrep -f Scribe.app/Contents/MacOS) | awk '{s+=$1} END {print s/1024 " Mo"}'
```

Objectif : < 120 Mo au total (app + WebKit + sidecar), CPU < 1 %. Le sidecar au repos ne fait
que surveiller Teams (un relevé toutes les 2 s) ; les modèles ne sont chargés qu'au `start`.

## Fait dans cet environnement (Linux)

- `cargo test --workspace --exclude scribe-app` : 60+ tests unitaires + 4 tests d'acceptation
  (§5.6 demandes sourcées / session / refus, §6.5 mot d'appel 18+/20 et 0 faux déclenchement,
  §7.3 compte rendu et corrélation, exclusions, confidentialité).
- Performance : recherche plein texte sur 500 réunions / 150 000 phrases : ~16 ms ; liste
  filtrée : ~75 ms (`cargo test -p scribe-core --release --test perf -- --ignored`).
- `scribe simulate fixtures/daily-45min.jsonl` : la daily de 45 min passe par tout le
  pipeline (sidecar simulé → réunion → 2 commandes vocales → compte rendu).
- App Tauri lancée sous Xvfb avec le vrai cœur, le sidecar simulé et le faux agent :
  assistant, réunion en direct, demandes, compte rendu, corrélation vérifiés à l'écran.
- UI : `npm run check` sans erreur, bundle ~104 Ko gzip (objectif < 300 Ko).
