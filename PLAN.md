# Scribe — assistant de réunion local (Mac + Teams) — plan v3

> Plan de réalisation destiné à des agents de code (Orca, Claude Code, Codex…).
> **Objectif** : l'équivalent de Granola en 100 % local, augmenté de recherches croisées
> codebase / Jira / Confluence / Datadog, déclenchables **à la voix** pendant la réunion,
> avec une **app de bureau légère en Rust** qui fonctionne sans Orca.
> **Contraintes** :
> - 0 € de coût en plus de l'abonnement IA que chaque dev a déjà (Claude Code, Codex, Gemini CLI, OpenCode…) ;
> - agent IA **configurable** : rien dans le code ne dépend d'un fournisseur ;
> - chaque dev compile l'outil lui-même (section 14) ;
> - pas d'Electron : app **Tauri 2 (Rust)**, empreinte mémoire et CPU minimale.

---

## 0. Comment utiliser ce plan

1. Créer un dépôt local `scribe/` (privé, jamais poussé sur un dépôt public), y déposer ce fichier.
2. Premier agent : « Lis PLAN.md, écris `docs/contracts.md` (schéma SQLite section 3 + protocole section 4.3), puis exécute la phase 0 et rends-moi `docs/spikes.md`. »
3. Après validation de la phase 0 : **4 agents en parallèle** (section 11).

Règles pour tous les agents :
- Français dans l'interface, les notifications et les comptes rendus.
- **Le Scribe ne publie jamais rien à l'extérieur** (Slack, Jira, Confluence, GitLab, Datadog). Il lit seulement ; tout ce qui sortirait est un brouillon que l'utilisateur valide lui-même.
- Aucun appel à une API payante. Les modèles ne sont appelés que via le **CLI officiel de l'agent choisi**, dans le cadre de l'abonnement du dev, jamais via un client maison qui réutiliserait un jeton.
- Aucun nom de fournisseur, de modèle ou d'option de CLI en dehors des adaptateurs (`crates/scribe-core/src/agents/`) et des réglages.
- Rien de propre à un dev dans le dépôt (équipe, chemins, mot d'appel, modèles) : tout est en base.

---

## 1. Vue d'ensemble

```
┌──────────────────────────── Scribe.app (Mac) ─────────────────────────────┐
│                                                                            │
│  ┌─ scribe-audio (sidecar Swift, binaire embarqué dans l'app) ──────────┐  │
│  │  Micro (AVAudioEngine + AEC)           → canal "me"   (Moi)          │  │
│  │  Process tap Core Audio sur Teams      → canal "them" (Eux)          │  │
│  │  Détection d'appel Teams, titre de fenêtre (Accessibilité)           │  │
│  │  VAD Silero + ASR Parakeet TDT v3 (FluidAudio, CoreML / ANE, FR)     │  │
│  │  ⇄ JSON lines sur stdin/stdout (section 4.3)                         │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                         │ segments                                         │
│  ┌─ scribe-core (Rust, dans le processus Tauri) ─────────────────────────┐ │
│  │  SQLite (WAL + FTS5) : réunions, transcription, demandes, comptes     │ │
│  │  rendus, décisions, actions, réglages                                 │ │
│  │  Détection du mot d'appel (canal "me" uniquement)                     │ │
│  │  File de tâches : demandes en direct, compte rendu, corrélation…      │ │
│  │  Adaptateurs d'agent → lance claude / codex / gemini / opencode…      │ │
│  │  Politique lecture seule commune                                      │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                                                                            │
│  ┌─ UI (Tauri 2 : WKWebView système + Svelte) ───────────────────────────┐ │
│  │  Icône de barre de menu · fenêtre Réunions / Recherche / Actions /    │ │
│  │  Réglages · vue « en direct »                                         │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────────┘
   scribe (CLI Rust) : ask · doctor · init · export · mcp (serveur MCP local)
          │ texte uniquement (jamais l'audio)
          ▼
   Agent IA choisi par le dev (son abonnement) + ses MCP (Atlassian, Datadog, GitLab…)

   Orca : facultatif. Ses agents peuvent interroger les réunions via `scribe mcp`.
```

**Principe de coût** : transcription continue en local ; l'IA n'est appelée que sur demande (voix ou texte), une fois en fin de réunion, et pour la corrélation si elle est activée.

---

## 2. Choix techniques (à confirmer en phase 0)

| Brique | Choix | Raison | Plan B |
|---|---|---|---|
| App de bureau | **Tauri 2** (Rust + WKWebView système), UI en **Svelte 5** + Vite, sans gros framework | Pas de Chromium embarqué : quelques dizaines de Mo de RAM au lieu de plusieurs centaines pour Electron | UI 100 % Rust native (Slint ou egui) si la WebView pose problème |
| Cœur | Rust stable, `tokio`, `rusqlite` (feature `bundled`, FTS5 inclus), `rusqlite_migration`, `serde`, `tracing` | Un seul langage pour l'app, la CLI et le serveur MCP | — |
| Serveur MCP | crate `rmcp` (SDK Rust officiel), transport stdio | Tous les agents parlent MCP | JSON-RPC stdio minimal fait main |
| Audio + ASR | **Sidecar Swift** `scribe-audio` : Core Audio process taps (`CATapDescription`, macOS 14.2+), `AVAudioEngine` avec voice processing, **FluidAudio** (Parakeet TDT v3 + Silero VAD, Apache 2.0) | Meilleures API Apple + transcription sur le Neural Engine, très peu de CPU | Tout en Rust : `objc2-core-audio` pour les taps + `whisper-rs` (Metal) ou `sherpa-onnx` (Parakeet ONNX) |
| Raccourcis globaux | `tauri-plugin-global-shortcut` (événements appui/relâchement) | Mode aparté « maintenir pour parler » | — |
| Lancement au démarrage | `tauri-plugin-autostart` | L'app vit dans la barre de menu | — |
| Rendu markdown | `marked` + `DOMPurify` (contenu produit par l'IA, donc toujours assaini) + CSP stricte | Sécurité | — |
| Diarisation des « Eux » | FluidAudio (pyannote), **en post-traitement** | Inutile en direct | Rester sur Moi/Eux |

Cibles de performance (critères d'acceptation) :
- App au repos (fenêtre fermée, pas de réunion) : **< 120 Mo de RAM** au total, CPU < 1 %.
- En réunion : CPU moyen < 25 % sur Apple Silicon (l'ASR tourne sur le Neural Engine).
- Ouverture de la fenêtre < 500 ms ; recherche plein texte sur 500 réunions < 200 ms.

Points d'attention :
- **Permissions macOS** : micro, « Enregistrement audio du système » (`NSAudioCaptureUsageDescription` dans l'`Info.plist`), Accessibilité (titre de la fenêtre Teams). Vérifier que la permission accordée à `Scribe.app` couvre bien le sidecar embarqué (spike S1).
- **Teams** : le nouveau Teams (bundle probable `com.microsoft.teams2`) peut sortir l'audio par un processus auxiliaire → taper tout l'arbre de processus Teams.
- **Mac géré par l'entreprise** : le MDM peut bloquer la permission audio système (spike S4).

---

## 3. Données — SQLite local

Fichier : `~/Library/Application Support/Scribe/scribe.db` (permissions 600 ; s'appuyer sur FileVault).
`PRAGMA journal_mode=WAL; foreign_keys=ON; busy_timeout=5000`. Migrations versionnées.
**Un seul écrivain principal** : `scribe-core` dans l'app. La CLI ouvre la base en lecture, et pour agir elle insère une tâche dans `jobs` (que l'app traite) ; si l'app ne tourne pas, la CLI exécute la tâche elle-même.

```sql
CREATE TABLE meetings (
  id TEXT PRIMARY KEY,                 -- ex. 2026-11-26-1030-daily-orion
  title TEXT, title_source TEXT,       -- teams_window | user | inferred
  started_at TEXT NOT NULL, ended_at TEXT,
  status TEXT NOT NULL,                -- live | ended | processing | done | error
  confidential INTEGER NOT NULL DEFAULT 0,   -- 1 = aucun appel IA pour cette réunion
  agent_provider TEXT, agent_session_id TEXT,
  correlation TEXT                     -- off | on_demand | auto (copie du réglage au démarrage, modifiable)
);

CREATE TABLE segments (
  id INTEGER PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL, ts TEXT NOT NULL, t0 REAL, t1 REAL,
  channel TEXT NOT NULL,               -- me | them
  speaker TEXT NOT NULL,               -- Moi | Eux | Eux-1…
  text TEXT NOT NULL,
  private INTEGER NOT NULL DEFAULT 0,  -- aparté : exclu du compte rendu
  is_echo INTEGER NOT NULL DEFAULT 0,
  request_id TEXT                      -- si l'énoncé est une commande
);
CREATE VIRTUAL TABLE segments_fts USING fts5(
  text, content='segments', content_rowid='id', tokenize='unicode61 remove_diacritics 2');

CREATE TABLE speakers (meeting_id TEXT, label TEXT, display_name TEXT, PRIMARY KEY (meeting_id, label));
CREATE TABLE notes (id INTEGER PRIMARY KEY, meeting_id TEXT, ts TEXT, text TEXT, source TEXT); -- user | voice

CREATE TABLE requests (
  id TEXT PRIMARY KEY, meeting_id TEXT, created_at TEXT,
  origin TEXT,                         -- voice | text | hotkey | cli
  intent TEXT,                         -- search | check | note | recap | cancel
  query TEXT, context_seconds INTEGER, tier TEXT,   -- quick | search | summary
  status TEXT, started_at TEXT, finished_at TEXT, error TEXT,
  summary TEXT,                        -- 2 lignes max (notification)
  answer_md TEXT, tools_used TEXT      -- JSON
);

CREATE TABLE reports (
  meeting_id TEXT PRIMARY KEY, markdown TEXT, generated_at TEXT,
  provider TEXT, model TEXT, correlated INTEGER DEFAULT 0, version INTEGER DEFAULT 1);
CREATE TABLE report_refs (             -- liens produits par la corrélation
  id INTEGER PRIMARY KEY, meeting_id TEXT, kind TEXT,  -- code | jira | confluence | datadog | gitlab | autre
  ref TEXT, title TEXT, url TEXT, verified INTEGER, note TEXT);
CREATE TABLE decisions (id INTEGER PRIMARY KEY, meeting_id TEXT, text TEXT, created_at TEXT);
CREATE TABLE actions (id INTEGER PRIMARY KEY, meeting_id TEXT, owner TEXT, text TEXT, due TEXT,
  status TEXT DEFAULT 'open');         -- open | done
CREATE VIRTUAL TABLE reports_fts USING fts5(markdown, content='reports', content_rowid='rowid',
  tokenize='unicode61 remove_diacritics 2');

CREATE TABLE jobs (
  id INTEGER PRIMARY KEY, type TEXT,   -- request | report | correlate | diarize | export
  meeting_id TEXT, payload TEXT, status TEXT, attempts INTEGER DEFAULT 0,
  created_at TEXT, started_at TEXT, finished_at TEXT, error TEXT);

CREATE TABLE repos (
  id INTEGER PRIMARY KEY, label TEXT, path TEXT UNIQUE,
  use_live INTEGER DEFAULT 1, use_correlation INTEGER DEFAULT 1, exclude_globs TEXT);
CREATE TABLE connectors (               -- MCP détectés chez l'agent choisi
  id INTEGER PRIMARY KEY, provider TEXT, name TEXT, status TEXT, detected_at TEXT,
  use_live INTEGER DEFAULT 0, use_correlation INTEGER DEFAULT 0,
  write_blockable INTEGER NOT NULL);    -- 0 = ne peut pas être activé (section 5.3)
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);   -- JSON
```

- **Audio** : jamais en base. Tampon temporaire dans `~/Library/Caches/Scribe/`, supprimé après traitement (réglage « conserver l'audio N jours », 0 par défaut).
- **Rétention** : réglage « supprimer les transcriptions après N jours » (vide = jamais). Les comptes rendus peuvent être gardés plus longtemps.
- **Sauvegarde** : copie quotidienne (`VACUUM INTO`) dans `backups/`, 7 dernières conservées.
- **Export** : markdown par réunion à la demande, ou export automatique vers un dossier (réglage, désactivé par défaut).

---

## 4. Composant A — `scribe-audio` (sidecar Swift)

### 4.1 Rôle
Uniquement audio → segments de texte. Aucune logique métier, aucun accès à la base.

### 4.2 Fonctions
1. **Détection d'appel** dans les apps de visio surveillées (Teams, Slack, Zoom, Webex, Discord, FaceTime, navigateur pour Meet ; liste réglable, message `call` par app) (sert au titre de la réunion et à **proposer** de démarrer par une notification ; l'écoute ne démarre **jamais** toute seule) : le processus Teams utilise le micro (`kAudioProcessPropertyIsRunningInput`, interrogé périodiquement) → événement `teams_call`. Titre lu dans la fenêtre de réunion via l'API d'accessibilité (`AXTitle`).
2. **Capture** de deux flux séparés, rééchantillonnés en 16 kHz mono : micro (voice processing activé, donc annulation d'écho) et tap sur l'arbre de processus Teams.
3. **VAD + ASR** : un énoncé est clos après 0,8 à 1,2 s de silence ou 25 s au maximum ; transcription Parakeet v3 ; **< 3 s** entre la fin de la phrase et l'émission du segment.
4. **Vocabulaire** : reçu au démarrage (mot d'appel, noms de services) ; injecté si le moteur le permet, sinon correction par dictionnaire après coup.
5. **Mode aparté** : tant que la commande `aside on` est active, les segments du micro sont marqués `private`.
6. **Robustesse** : changement de périphérique (AirPods), veille et réveil, redémarrage de Teams → recréer le tap sans perdre le fil.
7. **Test du mot d'appel** (section 6.4) : enregistre N secondes de micro et renvoie la transcription brute.

### 4.3 Protocole (JSON lines)
Sidecar → core (stdout) :
```json
{"type":"ready","version":"0.1.0","asr":"parakeet-tdt-v3"}
{"type":"teams_call","active":true,"title":"Daily ORION"}
{"type":"segment","ts":"2026-11-26T10:41:12+01:00","t0":668.2,"t1":672.9,"channel":"me","text":"Scribe, cherche…","private":false}
{"type":"level","me":-32.5,"them":-24.1}
{"type":"state","state":"listening|paused|stopped"}
{"type":"error","code":"tap_failed","message":"…","recoverable":true}
```
Core → sidecar (stdin) :
```json
{"cmd":"start","vocabulary":["Scribe","Atlas"],"silence_ms":1000}
{"cmd":"stop"}
{"cmd":"aside","on":true}
{"cmd":"wake_test","seconds":8}
```

### 4.4 Critères d'acceptation
- Appel Teams de 30 min : transcription complète, Moi et Eux séparés, aucun plantage.
- Débrancher et rebrancher le casque : reprise en moins de 5 s.
- Aucun fichier audio restant après traitement si la rétention vaut 0.

---

## 5. Composant B — `scribe-core` (Rust) : base, tâches, agents

### 5.1 Réception des segments
- Écrit les segments en base et alimente la vue « en direct ».
- **Anti-écho** : un segment `me` qui ressemble à plus de 80 % à un segment `them` dans une fenêtre de ±2 s → `is_echo = 1`, ignoré.
- **Détection du mot d'appel** sur les segments `me` non écho (section 6).

### 5.2 File de tâches
- Tâches `request`, `report`, `correlate`, `diarize`, `export`, avec 2 tâches IA en parallèle au maximum (réglable), délai maximal, et reprise au redémarrage de l'app.
- Pour une `request` : le prompt contient la demande, les `context_seconds` dernières secondes de transcription (pour résoudre « ça », « ce ticket ») et les notes.
- **Une session d'agent par réunion** quand l'agent sait reprendre une session (identifiant dans `meetings.agent_session_id`) ; sinon, l'adaptateur rejoue un résumé des échanges précédents.
- Réunion marquée `confidential` → **aucune tâche IA** n'est créée (seulement la transcription, et seulement si l'utilisateur la garde).
- Notifications macOS : « 🔎 Je cherche : … » puis « ✅ <résumé en 2 lignes> » (clic → ouvre la réponse dans l'app).

### 5.3 Adaptateurs d'agent (IA au choix)
```rust
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> &'static str;                 // "claude-code", "codex", "gemini", "opencode", "custom"
    async fn detect(&self) -> Detection;          // installé ? version ?
    fn capabilities(&self) -> Capabilities;       // resume_session, stream_json, read_only_files, tool_deny_list, extra_dirs
    async fn list_mcp(&self) -> Result<Vec<McpServer>>;            // pour l'écran Connecteurs
    async fn sync_instructions(&self, agent_dir: &Path) -> Result<()>; // génère CLAUDE.md / AGENTS.md / GEMINI.md
    async fn apply_policy(&self, policy: &ReadOnlyPolicy, run_dir: &Path) -> Result<()>;
    async fn run(&self, req: RunRequest, events: Sender<AgentEvent>) -> Result<RunResult>;
}
```

| Adaptateur | Lancement non interactif | Reprise | Consignes | Statut visé |
|---|---|---|---|---|
| `claude-code` | `claude -p` (`--model`, `--add-dir`, `--output-format stream-json`) | `--session-id` / `--resume` | `CLAUDE.md` + skills | **supporté** (référence) |
| `codex` | `codex exec` (bac à sable lecture seule, sortie JSON) | `codex exec resume` | `AGENTS.md` | supporté |
| `gemini` | `gemini -p` | selon la version | `GEMINI.md` | expérimental |
| `opencode` | `opencode run` | selon la version | `AGENTS.md` | expérimental |
| `custom` | modèle de commande saisi dans les réglages | non | fichier passé dans le prompt | générique |

Les options de ces CLI évoluent vite : l'agent qui implémente les **vérifie sur la version installée** (`--help`) et note les versions testées dans `docs/adapters.md`.

**Niveaux de modèle** : le code ne connaît que `quick` (notes, récap), `search` (recherche croisée) et `summary` (compte rendu). La correspondance avec les vrais modèles se fait dans les réglages (vide = modèle par défaut du CLI).

**Claude Code et MCP en mode `-p`** : les connecteurs du compte Claude sont censés être disponibles depuis la v2.1.83 (sauf avec `--bare`), mais des régressions ont été signalées → spike S3 ; plan B : MCP ajoutés localement (`claude mcp add --transport http …`).

### 5.4 Politique lecture seule commune
`agent/policy.toml` : dépôts lisibles (ceux cochés dans les réglages), aucun dossier inscriptible, commandes shell autorisées (`rg`, `git log|show|blame|grep`, `glab mr view|list`…), et motifs d'outils MCP interdits (`*send*`, `*post*`, `*create*`, `*update*`, `*edit*`, `*delete*`, `*transition*`, `*comment*`, `*merge*`, `*push*`). Chaque adaptateur la traduit dans le mécanisme de son CLI.
**Règle de sécurité** : si un adaptateur ne sait pas interdire les outils MCP en écriture, ses connecteurs sont marqués `write_blockable = 0` et **ne peuvent pas être activés** (case grisée dans l'UI, avec l'explication). L'agent ne fait que lire et répond sur sa sortie standard : c'est `scribe-core` qui écrit en base.

### 5.5 Consignes de l'agent (`agent/`)
- Une seule source : `agent/INSTRUCTIONS.md` + `agent/playbooks/*.md`, générés à la volée vers le format de chaque CLI dans un dossier d'exécution temporaire.
- Profil du dev (rôle, équipe), saisi dans les réglages et injecté dans les consignes.
- Règles : répondre en français, court en direct, **toujours citer ses sources** (ticket, page Confluence, `fichier:ligne`, lien Datadog), distinguer « vérifié » et « supposé ».
- Procédure `recherche-croisee` :
  1. extraire les entités (`APP-\d+` ou motif de clé Jira configurable, services, fonctions, endpoints, environnements, plages horaires) ;
  2. Jira (tickets cités + JQL sur les mots-clés) ;
  3. Confluence ;
  4. code : `rg`, `git log -S`, MR liées via `glab` en lecture ;
  5. Datadog : logs, monitors et incidents sur la fenêtre de temps pertinente ;
  6. synthèse : TL;DR de 2 lignes, constats par source, contradictions (« le ticket dit X, le code fait Y »), ce qui reste à vérifier.
  - Budget en direct : 15 appels d'outils au maximum (réglable), puis proposer « approfondir après la réunion ».
- Procédures `note` et `recap` : niveau `quick`, aucun outil externe.

### 5.6 Critères d'acceptation (testables sans audio, avec les fichiers d'exemple)
- Une demande texte « où est géré le verrou de créneau et quel ticket en parle ? » renvoie une réponse en moins de 90 s, avec au moins une source Jira et au moins une référence `fichier:ligne`.
- Une relance réutilise la session de la réunion.
- « Poste ça sur Slack » est refusé ; aucun outil d'écriture n'apparaît dans `tools_used`.
- Ces tests passent pour chaque adaptateur marqué « supporté ».

---

## 6. Commande vocale et mot d'appel

### 6.1 Deux modes
1. **À voix haute** (les participants entendent) : « Scribe, cherche les erreurs Datadog sur ms-booking depuis hier. »
2. **En aparté** : couper son micro dans Teams et **maintenir le raccourci d'aparté** (par défaut ⌃⌥Espace) pendant qu'on parle. Le Scribe capte le micro indépendamment de Teams, il n'y a pas besoin de mot d'appel, et ces phrases sont marquées `private`, donc exclues du compte rendu.

### 6.2 Mot d'appel configurable (Réglages → Voix)
- **Mot d'appel** : champ texte, « Scribe » par défaut. L'utilisateur choisit le sien.
- **Mode strict** (case à cocher) : exige un préfixe (« OK Scribe », « Dis Scribe ») pour réduire encore les faux déclenchements.
- **Variantes acceptées** : générées automatiquement (variantes phonétiques en français, erreurs de transcription courantes) et modifiables. Le mot est ajouté au vocabulaire de l'ASR.
- **Mots interdits comme variantes** : liste intégrée de mots courants en réunion de dev (« script », « description », « inscrit », « prescrire »…) + le vocabulaire de l'équipe. Une variante qui entre en conflit est refusée, avec l'explication.
- **Contrôles à la saisie** (avertissements, pas des blocages) :
  - moins de 2 syllabes, ou mot très courant en français → ⚠️ risque de faux déclenchements ;
  - **analyse de l'historique** : « ce mot apparaît 14 fois dans vos 20 dernières réunions sans être une commande » → ⚠️ ;
  - prénom d'un collègue repéré dans les transcriptions → ⚠️.
- **Bouton « Tester »** (section 6.4).
- **Retour sonore** discret quand une commande est détectée (réglable : aucun / son système / son dans le casque uniquement).

### 6.3 Règles de détection (mode à voix haute)
- **Uniquement sur le canal `me`** (et hors écho) : un participant ne peut jamais déclencher le Scribe.
- Le mot d'appel (ou le préfixe + le mot en mode strict) doit se trouver **dans les 2 premiers mots de la phrase**.
- Il doit être suivi d'un verbe reconnu ou d'une pause, puis de la demande :
  - `cherche|recherche|regarde|trouve` → `search`
  - `vérifie|confirme` → `check`
  - `note|ajoute une action|retiens` → `note` (niveau `quick`, aucune recherche)
  - `résume|récap` → `recap`
  - `annule` → `cancel` (dernière demande en cours)
  - `arrête l'écoute|termine la réunion|fin de la réunion` → `stop` (arrête l'écoute et lance le compte rendu)
  - Les verbes sont modifiables dans les réglages (fichier de grammaire en base).
- Fin de commande : on fusionne les phrases `me` qui se suivent à moins de 2 s d'écart, jusqu'à 1,5 s de silence.
- **Jamais de synthèse vocale dans l'audio de Teams.**

### 6.4 Bouton « Tester »
1. L'utilisateur prononce 5 fois « <mot d'appel>, cherche quelque chose » → taux de détection affiché (objectif : 5/5).
2. Il lit un court texte piège qui contient les mots interdits et des mots proches → objectif : 0 déclenchement.
3. Si le taux est insuffisant, l'app propose d'ajouter les transcriptions ratées comme variantes.

### 6.5 Critères d'acceptation
- Avec le mot par défaut, 20 commandes lues : au moins 18 détectées avec la bonne intention.
- 1 h d'audio de réunion contenant « script », « description », « inscrit » : **0 faux déclenchement**.
- La même commande dite par un participant (canal `them`) : 0 déclenchement. Avec des haut-parleurs au lieu d'un casque : 0 déclenchement dû à l'écho.
- Changer le mot d'appel dans les réglages prend effet sans redémarrage.

---

## 7. Compte rendu et corrélation

### 7.1 Compte rendu (fin de réunion, toujours, sauf réunion confidentielle)
Déclenché quand la réunion passe à `ended`, c'est-à-dire **uniquement à l'arrêt demandé par l'utilisateur** (menu de la barre d'état, raccourci, app, ou « Scribe, arrête l'écoute »). La fin de l'appel Teams n'arrête pas l'écoute.
1. Diarisation hors ligne du canal `them` (Eux-1, Eux-2…). Claude propose des noms quand ils sont prononcés (« merci Marie ») ; l'utilisateur valide dans l'app.
2. Agent, niveau `summary`, **sans outils externes** (rapide, ne consomme pas d'appels de recherche) :
   ```
   # <Titre> — <date>
   ## En bref (3 lignes)
   ## Décisions
   ## Actions — qui / quoi / échéance
   ## Questions ouvertes
   ## Points techniques évoqués
   ## Recherches faites pendant la réunion
   ## Mes notes (fusionnées, comme dans Granola)
   ```
   Les phrases `private` ne sont jamais utilisées.
3. Décisions et actions extraites dans leurs tables ; brouillons éventuels (commentaires Jira proposés…) rangés dans l'app, **jamais publiés automatiquement**.
4. Suppression de l'audio selon la rétention ; notification « 📝 Compte rendu prêt ».

### 7.2 Corrélation avec la codebase et les connecteurs (réglable)
- **Mode global** (Réglages → Corrélation) : `Désactivée` / `À la demande` (bouton « Corréler » sur le compte rendu) / `Automatique` (lancée juste après le compte rendu).
- **Sources** : cases à cocher par dépôt et par connecteur (`use_correlation`), indépendantes des sources utilisées en direct (`use_live`).
- **Exclusions** : ne jamais corréler les réunions dont le titre contient certains mots (par défaut : « 1:1 », « RH », « entretien », « perso ») ; jamais pour une réunion confidentielle.
- **Budget** : nombre maximal d'appels d'outils par corrélation (30 par défaut).
- **Ce que fait la tâche `correlate`** : reprend les « Points techniques évoqués » et les questions ouvertes, applique la procédure `recherche-croisee` sur les sources cochées, remplit `report_refs` (chaque lien marqué vérifié ou non), puis ajoute au compte rendu une section **« Liens avec le code et les outils »** avec des alertes (« la décision X contredit le comportement actuel de `fichier:ligne` »). Le compte rendu passe en `version + 1` ; la version précédente reste consultable.

### 7.3 Critères d'acceptation
- Transcription d'exemple de 45 min : compte rendu en moins de 2 min, au moins 1 décision et 1 action correctement attribuées, aucun contenu `private`.
- Corrélation désactivée : aucune tâche `correlate` n'est créée ; à la demande : seulement après un clic ; exclusions respectées (test sur une réunion « 1:1 »).

---

## 8. App de bureau (Tauri 2)

### 8.1 Barre de menu (toujours présente)
Icône dans la barre d'état macOS (à côté de l'heure), menu natif au clic : état (« ● Écoute en cours — titre » / « Appel Teams détecté : titre » / « En attente »), **Démarrer / Arrêter l'écoute**, « Réunion confidentielle », « Demande rapide… » (panneau : champ de demande, 5 dernières réponses), « Voir la transcription en direct », « Ouvrir Scribe », « Réglages… », « Quitter ». Un « ● » s'affiche à côté de l'icône pendant l'écoute.

### 8.2 Fenêtre principale
- **Réunions** : liste par date avec recherche plein texte (FTS5 sur transcriptions et comptes rendus), filtres (période, corrélée ou non, avec actions ouvertes).
- **Détail d'une réunion**, en onglets :
  - **Compte rendu** : markdown rendu, versions, bouton « Corréler », « Régénérer », export markdown ou copie ;
  - **Transcription** : horodatée, couleurs par intervenant, renommage des intervenants (Eux-1 → Marie), phrases privées masquées par défaut ;
  - **Demandes** : chaque demande avec sa réponse, ses sources et les outils utilisés ;
  - **Notes** : éditables.
  - Actions : renommer, marquer confidentielle, supprimer (avec confirmation).
- **En direct** : transcription qui défile, champ « Demander au Scribe », fil des demandes en cours.
- **Recherche** : plein texte dans toutes les réunions, plus « Demander à l'IA » (question sur l'historique, via l'agent et le serveur MCP Scribe).
- **Décisions & actions** : toutes les actions de toutes les réunions, case « fait », filtres par personne et par date.

### 8.3 Réglages
| Section | Contenu |
|---|---|
| **Agent IA** | Agent détecté parmi ceux installés (liste), modèle par niveau (`quick`, `search`, `summary`), modèle de commande pour `custom`, bouton « Tester l'agent », « Autoriser l'utilisation d'une clé API » (désactivé par défaut) |
| **Dépôts** | Ajouter un dossier (sélecteur), libellé, utilisé en direct / pour la corrélation, exclusions (globs) |
| **Connecteurs** | Liste des MCP de l'agent choisi (via `list_mcp`), état, cases direct / corrélation, ❌ grisé si l'agent ne sait pas bloquer leurs outils en écriture, bouton « Rafraîchir » |
| **Corrélation** | Mode (désactivée / à la demande / automatique), budget, mots d'exclusion |
| **Voix** | Mot d'appel, mode strict, variantes, verbes, retour sonore, bouton « Tester » (section 6) |
| **Audio & transcription** | Micro, détection automatique de Teams, silence de fin de phrase, vocabulaire (noms de services, équipe), raccourcis (démarrer/arrêter, aparté, note rapide) |
| **Confidentialité** | Rétention de l'audio et des transcriptions, export automatique, réunions exclues, « Tout supprimer » |
| **Profil** | Rôle, équipe, motif des clés Jira |
| **Import / export** | Exporter / importer les réglages en TOML (pour partager une config de départ dans l'équipe, sans chemins ni données perso) |

### 8.4 Contraintes UI
Pas de framework lourd ; bundle front < 300 Ko gzip ; thème clair et sombre suivant le système ; navigation au clavier ; aucune requête réseau depuis la WebView (CSP `default-src 'self'`).

---

## 9. CLI, serveur MCP et Orca (facultatif)

- `scribe ask "qu'a-t-on décidé sur l'export Brindille ?"` : recherche dans l'historique, puis éventuellement une recherche croisée.
- `scribe doctor` : permissions macOS, Teams, sidecar, agent et version, MCP visibles en mode non interactif, et **test automatique de la politique lecture seule** (« poste sur Slack » doit échouer) → ✅ / ⚠️ / ❌ avec la marche à suivre.
- `scribe init` : premier lancement en ligne de commande (équivalent de l'assistant de l'app).
- `scribe export <id|--all> --format md`.
- `scribe mcp` : **serveur MCP local (stdio, lecture seule)** pour n'importe quel agent, y compris dans Orca :
  - `search_meetings(query, from?, to?)`, `get_meeting(id)`, `get_transcript(id, from_ts?, to_ts?)` (sans les phrases privées), `get_report(id)`, `list_decisions(since?)`, `list_actions(status?, owner?)`.
  - Pour l'utiliser dans un agent : `claude mcp add scribe -- scribe mcp` (ou l'équivalent du CLI).
- **Orca n'est plus nécessaire** ; pour ceux qui l'utilisent, `scribe mcp` donne accès aux réunions depuis leurs agents.

---

## 10. Phases

### Phase 0 — Spikes (1 j) : **bloquant**
| Spike | Livrable | Critère |
|---|---|---|
| S1 Capture | sidecar minimal qui écrit `me.wav` + `them.wav` pendant un vrai appel Teams, **lancé depuis une app Tauri vide** | deux pistes audibles, permissions accordées à l'app et valables pour le sidecar |
| S2 ASR | transcription de S1 avec FluidAudio/Parakeet v3 (comparaison avec WhisperKit) | français lisible, noms techniques compris, < 1 s par phrase |
| S3 Agents | demander à Claude Code puis à Codex, en mode non interactif, de lister leurs outils MCP ; tenter un outil d'écriture interdit | MCP visibles ; écriture bloquée ; sinon plan B documenté |
| S4 Poste de travail | permissions micro, audio système et accessibilité sur le Mac pro | accordées ; sinon arrêter et prévenir l'utilisateur |
| S5 Empreinte | app Tauri vide + barre de menu, mesure de la RAM au repos | < 120 Mo |

### Phase 1 — Socle
`scribe-core` (schéma, migrations, file de tâches), sidecar (capture + transcription), vue « en direct » minimale.

### Phase 2 — Compte rendu + historique
Compte rendu de fin de réunion, écrans Réunions / Détail, recherche plein texte.

### Phase 3 — Agents et recherche croisée
Adaptateurs `claude-code` puis `codex`, politique lecture seule, demandes tapées, écrans Agent IA / Dépôts / Connecteurs.

### Phase 4 — Voix
Détection du mot d'appel, mode aparté, écran Voix avec le bouton « Tester ».

### Phase 5 — Corrélation + finitions
Tâche `correlate`, écran Corrélation, décisions et actions, CLI `ask` / `export`, serveur `scribe mcp`, import/export des réglages.

### Phase 6 — Équipe
`make install`, assistant de premier lancement, `scribe doctor` complet, adaptateurs `gemini` et `opencode` (expérimentaux), README.

---

## 11. Découpage en agents (après la phase 0)

Prérequis commun, fait par le premier agent : `docs/contracts.md` (schéma SQL + protocole du sidecar + trait `AgentAdapter`) et `fixtures/` avec une **transcription synthétique en français** de 45 min (daily fictif : tickets, un service `ms-*`, une décision, deux actions, une commande vocale, une phrase privée, des mots pièges comme « script »).

| Worktree | Agent | Périmètre | Testable sans |
|---|---|---|---|
| `scribe-audio` | A | `audio/` (sidecar Swift) | base, UI |
| `scribe-core` | B | `crates/scribe-core` : base, tâches, adaptateurs, politique, mot d'appel | audio (fichiers d'exemple) |
| `scribe-post` | C | compte rendu, corrélation, `crates/scribe-cli` (ask, export, mcp, doctor) | audio, UI |
| `scribe-app` | D | `app/` (Tauri + Svelte) : barre de menu, écrans, réglages | audio (données d'exemple en base) |

Arborescence :
```
scribe/
  PLAN.md
  docs/{contracts.md, spikes.md, adapters.md}
  audio/ScribeAudio/            # Swift package, exécutable sidecar
  crates/scribe-core/           # lib Rust : db, jobs, agents/, policy, wake/
  crates/scribe-cli/            # bin `scribe` : ask, doctor, init, export, mcp
  app/src-tauri/                # Rust (dépend de scribe-core), embarque le sidecar
  app/ui/                       # Svelte 5 + Vite
  agent/                        # INSTRUCTIONS.md, playbooks/, policy.toml
  fixtures/
  Makefile
```

---

## 12. Coûts et quota

- Audio, transcription, diarisation, base, app : **local, 0 €**.
- IA : uniquement via le CLI officiel de l'agent choisi, donc dans le cadre de l'abonnement de chaque dev. Ordres de grandeur : 1 h de transcription ≈ 15 000 tokens ; ce qui consomme le plus, ce sont les **recherches croisées** et la **corrélation**, d'où leurs budgets et le mode « à la demande ».
- Garde-fous : niveau `quick` pour `note` et `recap`, compte rendu sans outils, 2 tâches IA en parallèle au maximum, pas de résumé en continu.
- Clé API : jamais, sauf activation explicite dans les réglages.
- Distribution : compilation locale avec signature ad hoc, donc pas de compte développeur Apple payant.

---

## 13. Risques et garde-fous

| Risque | Mesure |
|---|---|
| Le MDM bloque la permission audio système | Spike S4 en premier ; arrêter et prévenir si c'est bloqué |
| La permission accordée à l'app ne couvre pas le sidecar | Spike S1 ; plan B : capture en Rust (`objc2-core-audio`) dans le processus principal |
| Teams sort l'audio par un processus auxiliaire | Taper tout l'arbre de processus Teams ; plan B : tap global en excluant Scribe |
| Écho (haut-parleurs) | AEC + déduplication + casque recommandé |
| Faux déclenchements | Mot d'appel en début de phrase + verbe, mode strict, mots interdits, analyse de l'historique, canal `me` seul, tests |
| Un CLI d'agent change ses options | Adaptateurs isolés, versions testées notées, `scribe doctor` signale tout écart |
| Un agent ne sait pas bloquer les MCP en écriture | Connecteurs non activables pour cet adaptateur |
| L'agent écrit à l'extérieur | Politique en refus explicite + test automatique dans `doctor` et dans la CI |
| Contenu IA malveillant dans l'UI | Markdown assaini (DOMPurify) + CSP stricte |
| Confidentialité | Audio jamais envoyé ni stocké durablement ; base locale en 600 ; réunions confidentielles sans IA ; rétention réglable |
| Consentement | Informer les participants que la réunion est transcrite (RGPD) ; pour un usage à l'échelle de l'entreprise : validation sécurité / RGPD (et probablement CSE) |

---

## 14. Utilisation par d'autres devs (compilation sur leur poste)

Objectif : cloner, compiler et être opérationnel en moins de 15 minutes.

- **Prérequis** (README) : macOS 14.2+ sur Apple Silicon, Xcode (pour le sidecar Swift), Rust stable (`rustup`), Node 22 (seulement pour compiler l'UI), au moins un CLI d'agent installé et connecté.
- **`make install`** : compile le sidecar, l'UI et l'app Tauri, applique la signature ad hoc, copie `Scribe.app` dans `~/Applications`, installe la CLI `scribe`, télécharge les modèles audio.
- **Assistant de premier lancement** (dans l'app, ou `scribe init`) :
  1. permissions macOS, guidées étape par étape ;
  2. choix de l'agent parmi ceux détectés, puis test ;
  3. dépôts à rendre lisibles ;
  4. connecteurs à activer (seulement ceux dont l'écriture peut être bloquée) ;
  5. mot d'appel, puis test ;
  6. mode de corrélation (par défaut : à la demande).
- **Config d'équipe** : un fichier TOML de réglages de départ, sans chemins ni données perso (vocabulaire des services, verbes, exclusions), importable depuis l'écran Réglages.
- **Mise à jour** : `git pull && make install` ; migrations de base automatiques, avec sauvegarde juste avant.
