<p align="center"><img src="app/src-tauri/icons/128x128.png" width="96" alt=""></p>

# Scribe

Assistant de réunion **100 % local** pour Mac (Teams, Slack, Zoom, Webex, Discord, FaceTime, Google Meet…) : transcription en direct sur le Neural
Engine, compte rendu de fin de réunion, et recherches croisées **à la voix** dans le code, Jira,
Confluence et Datadog — avec l'agent IA que vous avez déjà (Claude Code, Codex, Gemini CLI,
OpenCode…), dans le cadre de votre abonnement. Aucun coût en plus, aucune donnée audio qui sort
du Mac, et **le Scribe ne publie jamais rien** : il lit, vous publiez.

![Compte rendu](docs/screenshots/detail-compte-rendu-clair.png)

> ⚠️ **État** : le code est complet, mais les spikes de la phase 0 (capture Teams, transcription,
> permissions macOS, empreinte mémoire) doivent encore être faits sur un Mac.
> Voir [`docs/spikes.md`](docs/spikes.md).

## Ce que fait Scribe

- **Écoute 100 % manuelle** : rien n'est jamais enregistré sans votre action. Quand un appel
  commence dans une app de visio (Teams, Slack, Zoom…), Scribe le **propose** (notification) ; vous démarrez l'écoute depuis l'icône
  de la barre d'état (en haut à droite, à côté de l'heure), ⌃⌥S ou l'app. Elle ne s'arrête
  pas toute seule non plus : menu, ⌃⌥S, ou à la voix « *Scribe, arrête l'écoute* ».
- **Transcription en direct** de vos appels, en séparant votre voix (« Moi », micro) et celle
  des autres (« Eux », son de l'app de l'appel), sans enregistrer d'audio durablement.
- **Multi-apps** : Teams, Slack (huddles), Zoom, Webex, Discord, FaceTime sont surveillés par
  défaut ; Google Meet via le navigateur est activable (il capte tout le son du navigateur) ;
  toute autre app peut être ajoutée par son identifiant de bundle (Réglages → Audio).
- **Commandes vocales** : « *Scribe, cherche les erreurs Datadog sur ms-booking depuis hier.* »
  — ou en **aparté** : micro coupé dans l'app d'appel, maintenez ⌃⌥Espace et parlez. Notification
  « 🔎 Je cherche… » puis « ✅ réponse en 2 lignes », réponse complète et sourcée dans l'app.
- **Compte rendu** automatique (décisions, actions qui/quoi/échéance, questions ouvertes,
  points techniques, vos notes fusionnées), les apartés n'y figurent jamais.
- **Corrélation** (désactivée / à la demande / automatique) : le compte rendu est croisé avec
  vos dépôts et connecteurs, avec des alertes quand une décision contredit le code.
- **Historique** : recherche plein texte dans toutes les réunions, décisions et actions,
  questions en langage naturel, et serveur MCP `scribe mcp` pour vos agents.

## Installation (≈ 15 min)

Prérequis : macOS 14.2+ sur Apple Silicon, Xcode, Rust stable (`rustup`), Node 22 (seulement
pour compiler l'interface), et au moins un CLI d'agent installé et connecté (`claude`, `codex`…).

```bash
git clone <ce dépôt> scribe && cd scribe
make install          # compile le sidecar, l'UI et l'app ; signature ad hoc ; ~/Applications/Scribe.app
                      # + CLI dans ~/.local/bin/scribe + téléchargement des modèles audio (~1 Go)
open ~/Applications/Scribe.app
```

Au premier lancement, l'assistant vous guide : permissions macOS → choix de l'agent et test →
dépôts lisibles → connecteurs (seulement ceux dont l'écriture peut être bloquée) → mot d'appel
et test vocal → mode de corrélation. Équivalent en ligne de commande : `scribe init`.
Vérification à tout moment : `scribe doctor` (`--live` pour tester réellement que l'agent
refuse de publier).

Mise à jour : `git pull && make install` (sauvegarde de la base puis migrations automatiques).

## Utilisation

| Où | Quoi |
|---|---|
| Barre d'état (icône à côté de l'heure) | menu : état (« ● Écoute en cours », « Appel Slack détecté »…), **Démarrer / Arrêter l'écoute**, réunion confidentielle, demande rapide, transcription en direct, ouvrir Scribe, réglages |
| Fenêtre | Réunions (compte rendu, transcription, demandes, notes), En direct, Recherche, Décisions & actions, Réglages |
| Voix | « *Scribe, cherche / vérifie / note / résume / annule …* », « *Scribe, arrête l'écoute* » (mot d'appel et verbes réglables) |
| Raccourcis | ⌃⌥S démarrer/arrêter · ⌃⌥Espace aparté (maintenir) · ⌃⌥N note rapide |
| CLI | `scribe ask "qu'a-t-on décidé sur l'export Brindille ?"` · `scribe export <id>` · `scribe doctor` · `scribe mcp` |

Donner accès aux réunions à un agent (lecture seule) : `claude mcp add scribe -- scribe mcp`.

Réunion **confidentielle** : aucune IA (ni recherche, ni compte rendu). Réunions dont le titre
contient « 1:1 », « RH », « entretien », « perso » : jamais corrélées (réglable).

## Confidentialité

- Audio : jamais envoyé, tampon temporaire supprimé après le compte rendu (rétention réglable,
  0 jour par défaut).
- Base locale `~/Library/Application Support/Scribe/scribe.db` (droits 600, FileVault),
  sauvegarde quotidienne (7 dernières), rétention des transcriptions réglable.
- Seul du **texte** est envoyé à l'agent IA que vous avez choisi, et seulement à votre demande,
  en fin de réunion, ou pour la corrélation.
- Les clés d'API de l'environnement ne sont pas transmises à l'agent (abonnement uniquement),
  sauf activation explicite.
- Pensez à informer les participants que la réunion est transcrite (RGPD).

## Architecture

```
audio/ScribeAudio/     sidecar Swift : Core Audio process tap (app de l'appel) + micro (AEC),
                       VAD Silero + ASR Parakeet v3 (FluidAudio, Neural Engine) → JSON lines
crates/scribe-core/    Rust : SQLite (WAL + FTS5), file de tâches, mot d'appel, anti-écho,
                       adaptateurs d'agent, politique lecture seule, compte rendu, corrélation
crates/scribe-cli/     bin `scribe` : ask, doctor, init, export, mcp, replay, simulate
app/src-tauri/         app Tauri 2 (barre de menu, notifications, raccourcis globaux)
app/ui/                Svelte 5 + Vite (~104 Ko gzip), marked + DOMPurify, CSP stricte
agent/                 consignes de l'agent, procédures (recherche croisée…), policy.toml
fixtures/              daily synthétique de 45 min + faux agent pour les tests
docs/                  contracts.md (interfaces), adapters.md, spikes.md
```

Sécurité de l'agent (détail dans [`docs/adapters.md`](docs/adapters.md)) : l'agent ne fait que
lire. Pour Claude Code, chaque appel d'outil passe par un crochet qui refuse les outils MCP
d'écriture (`*send*`, `*create*`, `*comment*`, `executeWrite`…), les commandes shell hors liste
et les lectures hors des dépôts cochés. Un agent qui ne sait pas bloquer ses outils MCP voit ses
connecteurs grisés.

## Développement

```bash
make test        # tous les tests : Rust (unitaires, fonctionnels, acceptation, perf), UI (Vitest), e2e (Playwright)
make serve       # UI + vrai cœur dans le navigateur : http://127.0.0.1:4180/?backend=http
make simulate    # rejoue la daily d'exemple dans tout le pipeline (faux agent, sans Mac)
make dev         # app avec rechargement à chaud, sidecar simulé, faux agent
cd app/ui && npm run dev   # l'interface seule dans un navigateur, avec données d'exemple
```

Les tests e2e « réels » pilotent l'interface dans Chromium contre `scribe serve` (même API
que l'app Tauri, en HTTP), le simulateur de sidecar et un faux CLI `claude` : démarrage
manuel, transcription, demandes vocales et tapées, refus de publier, arrêt, compte rendu,
corrélation, recherche, export. Reprise du projet : [`docs/HANDOFF.md`](docs/HANDOFF.md).

Variables utiles : `SCRIBE_HOME` (dossier des données), `SCRIBE_SIDECAR` (commande du sidecar,
ex. `scribe replay fixtures/daily-45min.jsonl --speed 20`), `RUST_LOG`.
Le plan de réalisation complet est dans [`PLAN.md`](PLAN.md) ; les interfaces entre composants
dans [`docs/contracts.md`](docs/contracts.md).

---

<sub>Développé par Nakile Solutions.</sub>
