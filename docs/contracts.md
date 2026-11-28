# Contrats entre composants

Ce document fige les interfaces entre les quatre périmètres (sidecar audio, cœur Rust,
post-traitement/CLI, app Tauri). Toute évolution passe par une mise à jour de ce fichier.

Sommaire :
1. Schéma SQLite
2. Protocole du sidecar `steno-audio` (JSON lines)
3. Trait `AgentAdapter`
4. Réglages (JSON en base, TOML en import/export)
5. API de l'app : commandes Tauri et événements
6. Fichiers d'exemple (`fixtures/`)
7. Backend HTTP (`steno serve`) : tests e2e et développement sans Tauri
8. App de bureau : actions en ligne de commande

> **Règle produit** : l'écoute ne démarre **jamais** automatiquement et ne s'arrête **jamais**
> automatiquement. Un appel détecté (Teams, Slack, Zoom…) donne au plus une notification qui propose de
> démarrer ; l'arrêt se fait par l'utilisateur (menu de la barre d'état, raccourci, app) ou à
> la voix (« Sténo, arrête l'écoute », intention `stop`).

---

## 1. Schéma SQLite

Fichier : `~/Library/Application Support/Steno/steno.db` sur macOS,
`~/.local/share/Steno/steno.db` sous Linux (variable d'environnement `STENO_HOME` pour changer le
dossier, utilisée par les tests).
Pragmas à chaque ouverture : `journal_mode=WAL`, `foreign_keys=ON`, `busy_timeout=5000`.
Permissions du fichier : `0600`. Migrations : `crates/steno-core/src/db/migrations/NNN_*.sql`,
appliquées par `rusqlite_migration` (`user_version`). Une sauvegarde `VACUUM INTO` est faite
avant toute migration d'une base non vide.

Horodatages : texte ISO 8601 avec fuseau (`2026-11-26T10:41:12+01:00`). Booléens : `INTEGER` 0/1.

Le schéma de référence est celui de `PLAN.md` §3, avec ces **ajouts** :

```sql
-- Historique des versions du compte rendu (la version courante reste dans `reports`).
CREATE TABLE report_versions (
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  version INTEGER NOT NULL,
  markdown TEXT NOT NULL, generated_at TEXT, provider TEXT, model TEXT,
  correlated INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (meeting_id, version));

-- Brouillons produits par l'IA (commentaire Jira proposé, message…), jamais publiés.
CREATE TABLE drafts (
  id INTEGER PRIMARY KEY, meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,
  kind TEXT, target TEXT, body TEXT, created_at TEXT);

-- État d'exécution partagé app <-> CLI (battement de cœur de l'app, etc.).
CREATE TABLE runtime (key TEXT PRIMARY KEY, value TEXT, updated_at TEXT);
```

- Clés étrangères `ON DELETE CASCADE` sur toutes les tables filles de `meetings`
  (`segments`, `speakers`, `notes`, `requests`, `reports`, `report_refs`, `decisions`,
  `actions`, `report_versions`, `drafts`). `jobs.meeting_id` n'a pas de clé étrangère.
- Index : `segments(meeting_id, seq)`, `requests(meeting_id, created_at)`,
  `jobs(status, id)`, `actions(status)`, `meetings(started_at)`.
- FTS5 `segments_fts` et `reports_fts` sont des tables à contenu externe : elles sont tenues
  à jour par des **triggers** `AFTER INSERT / UPDATE / DELETE`.
- `jobs.status` : `pending | running | done | error | cancelled`. Au démarrage de l'app,
  les tâches `running` repassent en `pending` (`attempts + 1`, abandon après 3 essais).
- `requests.status` : `queued | running | done | error | cancelled | refused`.
- `runtime` clé `app_heartbeat` : mise à jour toutes les 5 s par l'app. La CLI considère
  que l'app tourne si le battement a moins de 15 s ; sinon elle exécute la tâche elle-même.
  A single process owns the heartbeat: a core does not overwrite another process's fresh
  heartbeat (`steno serve` next to the app), takes it over once it is stale, and only
  marks it stopped on shutdown if it owns it. A core starting while another process's
  heartbeat is fresh leaves the meeting marked `live` alone (it belongs to that process);
  otherwise it closes it (crash recovery).
- `jobs.type = 'control'`: « arrête l'écoute » or « annule » asked from a process that does
  not own the heartbeat (`steno ask`) while another one does. The request stays `queued`
  and the owner runs it (live session, agents' cancellation tokens). Not an AI job: it
  never waits behind `agent.max_parallel`.
- `jobs.type = 'diarize'`: retranscription with the final model, then diarization, of a
  finished meeting. One runs at a time (both load large models in the audio sidecar and
  read the whole recording); the others stay `pending` until it ends. Not an AI job: it
  never waits behind `agent.max_parallel`.
- `jobs.status`: a `request` job abandoned after 3 attempts also moves its request to
  `error`. A request is only set `running` from `queued` or `running` (never after a
  cancellation).
- `runtime` key `settings_rev`: new random value on every settings save. A running core
  checks it every 5 s and reloads the settings (`steno://settings`) when another process
  (`steno settings import`) saved them.
- Write transactions use `BEGIN IMMEDIATE` (the app and the CLI share the database).

Migrations after `001_init.sql` (the schema above):

| Migration | Change |
|---|---|
| `002_call_app.sql` | `meetings.call_app TEXT`: call app of the meeting (`teams`, `slack`, `zoom`…). |
| `003_context.sql` | `meeting_context(meeting_id PK, text, template, updated_at)`: free-text context and report template of a meeting; `context_files(id, meeting_id, name, kind, size, text, truncated, added_at)`: attached documents (extracted text), index `context_files(meeting_id, id)`. |
| `004_job_not_before.sql` | `jobs.not_before TEXT` (UTC, RFC 3339): a job waits until that time (delay left after the meeting to add context before the report is written). |
| `005_planning.sql` | `meeting_context.event_id TEXT`: link to the calendar event; `meeting_participants(id, meeting_id, name, email, source DEFAULT 'manual')`: invited participants, index `meeting_participants(meeting_id, id)`. Planned meetings use `meetings.status = 'planned'`, with `started_at` set to the scheduled time. |

The three new tables are children of `meetings` with `ON DELETE CASCADE`.

---

## 2. Protocole du sidecar `steno-audio`

Transport : le sidecar est lancé par le cœur ; une ligne JSON par message sur stdout
(sidecar → cœur) et stdin (cœur → sidecar), UTF-8, terminée par `\n`. Les journaux du
sidecar vont sur **stderr** uniquement. Le cœur ignore les champs inconnus et les types
inconnus (compatibilité ascendante).

### Sidecar → cœur

```json
{"type":"ready","version":"0.1.0","asr":"parakeet-tdt-v3"}
{"type":"permissions","microphone":"granted","system_audio":"unknown","accessibility":"denied","calendar":"unknown"}
{"type":"call","active":true,"app":"slack","title":"Huddle dans #equipe-flotte"}
{"type":"teams_call","active":true,"title":"Daily ORION"}
{"type":"segment","ts":"2026-11-26T10:41:12+01:00","t0":668.2,"t1":672.9,"channel":"me","text":"Sténo, cherche…","private":false}
{"type":"level","me":-32.5,"them":-24.1}
{"type":"devices","inputs":[{"id":"BuiltInMicrophoneDevice","name":"Micro du MacBook Pro","default":true,"transport":"builtin"}],"outputs":[{"id":"AirPodsUID","name":"AirPods Pro","default":true,"transport":"bluetooth"}]}
{"type":"state","state":"listening"}
{"type":"wake_test","text":"steno cherche quelque chose"}
{"type":"diarization","meeting_id":"…","turns":[{"t0":12.0,"t1":18.5,"speaker":"S1"}]}
{"type":"retranscription","meeting_id":"…","items":[{"id":42,"text":"Ce deuxième sujet…"}]}
{"type":"calendar","events":[{"id":"…","title":"Revue Nova","start":"…","end":"…","all_day":false,"attendees":[{"name":"Inès","email":"ines@exemple.fr","status":"accepted","organizer":false,"me":false}]}]}
{"type":"error","code":"tap_failed","message":"…","recoverable":true}
```

- `state` : `listening | paused | stopped`.
- `permissions` : chaque valeur vaut `granted | denied | unknown` ; émis après `ready`
  et en réponse à `{"cmd":"permissions"}`.
  `calendar` is the optional calendar access (EventKit on macOS) used by `calendar_events`.
- `t0` / `t1` : secondes depuis le `start` (horloge de la session de capture).
- `segment.private` : `true` pendant un aparté (seulement pour `channel = "me"`).
- `segment.speaker` (facultatif, `channel = "them"`, non émis aujourd'hui : les intervenants sont identifiés en fin de réunion seulement) : intervenant reconnu (« S1 »…) ; le cœur l'enregistrerait comme « Eux-1 »…
- `call` : un appel commence ou se termine dans une app surveillée (voir `watch`). `app` est
  l'identifiant de l'app (`teams`, `slack`, `zoom`…) ; `title` peut être `null`
  (accessibilité refusée ou titre introuvable). Un événement par app : plusieurs appels
  peuvent être actifs en même temps.
- `teams_call` : **ancien format**, toujours accepté par le cœur ; équivaut à
  `{"type":"call","app":"teams",…}`.
- `devices` : micros (`inputs`) et sorties (`outputs`) disponibles. `id` = UID Core Audio
  (stable d'un branchement à l'autre), `default` = périphérique par défaut du système,
  `transport` = `builtin | usb | bluetooth | virtual | aggregate | other`. Émis après `ready`,
  à chaque branchement / débranchement, et en réponse à `{"cmd":"devices"}`.
- Code `device_missing` (récupérable) : le micro demandé est absent ; le sidecar utilise le
  micro par défaut et le dit dans `message`.
- Codes d'erreur : `mic_denied`, `system_audio_denied`, `tap_failed`, `device_changed`, `device_missing`,
  `asr_failed`, `model_missing`, `bad_command`.

### Cœur → sidecar

```json
{"cmd":"watch","apps":[{"id":"teams","name":"Microsoft Teams","bundles":["com.microsoft.teams2","com.microsoft.teams"]},{"id":"slack","name":"Slack","bundles":["com.tinyspeck.slackmacgap"]}]}
{"cmd":"start","vocabulary":["Sténo","Atlas"],"silence_ms":1000,"keep_audio":false,"audio_dir":"/…/Caches/Steno/<meeting_id>","apps":["slack"],"input_device":null}
{"cmd":"start",…,"apps":[],"room":true}   // présentiel : micro seul, sans tap ni voice processing
{"cmd":"set_input","device":"BuiltInMicrophoneDevice"}
{"cmd":"devices"}
{"cmd":"play","sound":"detect","device":null}
{"cmd":"stop"}
{"cmd":"aside","on":true}
{"cmd":"pause"}             // capture en pause : silence sur les deux canaux (`state: paused`)
{"cmd":"resume"}            // reprise (`state: listening`)
{"cmd":"mute","on":true}     // micro coupé dans Sténo : silence à la place du micro
{"cmd":"wake_test","seconds":8}
{"cmd":"diarize","meeting_id":"…","audio_dir":"/…/Caches/Steno/<meeting_id>"}
{"cmd":"diarize","meeting_id":"…","audio_dir":"…","file":"me.wav"}   // présentiel : le micro (défaut : them.wav)
{"cmd":"retranscribe","meeting_id":"…","audio_dir":"…","language":"fr","vocabulary":["Atlas"],
 "items":[{"id":42,"file":"them.wav","t0":12.0,"t1":18.5}]}
{"cmd":"calendar_events","from":"2026-11-27T08:00:00+01:00","to":"2026-12-04T08:00:00+01:00"}   // permission « calendar » (facultative)
{"cmd":"permissions"}
{"cmd":"request_permission","kind":"microphone"}   // ou "system_audio", "accessibility", "calendar"
{"cmd":"shutdown"}
```

- `watch` : liste des apps de visio à surveiller (identifiant, nom, préfixes de bundle ;
  un processus dont le bundle **commence par** un préfixe appartient à l'app, auxiliaires
  compris). Envoyé par le cœur juste après `ready` puis à chaque changement de réglages.
  Avant le premier `watch`, le sidecar surveille seulement Teams.
- `start.apps` : identifiants des apps dont le son est capté (canal `them`) — l'app de
  l'appel en cours si le cœur en connaît une, sinon toutes les apps surveillées. Absent ou
  vide = toutes les apps surveillées.
- `start.input_device` / `set_input.device` : UID du micro à utiliser (`null` = micro par
  défaut du système, en suivant ses changements). `set_input` change de micro pendant la
  capture sans l'interrompre (au plus un court trou, horloge `t0/t1` conservée).
- `play` : joue le signal discret de détection d'une commande (`sound` : `detect`) sur la sortie
  `device` (`null` = sortie par défaut). Jamais injecté dans le son de l'app d'appel.

#### Sidecar Linux (`crates/steno-audio-linux`)

Même protocole ; ce qui diffère :

- **`watch`** : chaque entrée de `bundles` est un préfixe (sans casse) comparé au nom de
  l'exécutable du processus audio (`application.process.binary`, `/proc/<pid>/comm`, `exe`), à
  son identifiant Flatpak (`.flatpak-info`) ou Snap (cgroup), puis aux ancêtres du processus ;
  en dernier recours au nom d'application annoncé au serveur son (jamais pour un navigateur).
- **Appel** : une app est en appel quand l'un de ses processus enregistre un vrai micro (un
  flux qui enregistre le moniteur d'une sortie ne compte pas).
- **Capture `them`** : moniteur de chacun des flux de lecture de l'app (sink inputs), mélangés ;
  les flux qui apparaissent pendant la capture sont ajoutés aussitôt.
- **Identifiants de périphériques** : noms des nœuds PulseAudio (`alsa_input.usb-…`).
  `transport` d'après `device.bus` / `device.api` (`usb`, `bluetooth`), `builtin` pour les
  cartes internes, `virtual` pour les sorties nulles, filtres et annulations d'écho,
  `aggregate` pour une sortie combinée, `other` (HDMI…).
- **`permissions`** : pas d'autorisation système. `microphone` et `system_audio` valent
  `granted` dès que le serveur son répond (`unknown` sinon) ; `accessibility` vaut `granted` si
  X11 est joignable (titres des fenêtres), `unknown` sinon ; `calendar` vaut `granted` si une
  source de calendrier existe (Evolution Data Server ou `calendars.txt`), `unknown` sinon.
  `request_permission` réémet simplement `permissions`. `mic_denied` et
  `system_audio_denied` ne sont jamais émis.
- **`calendar_events`** : cache local d'Evolution Data Server
  (`~/.cache/evolution/calendar/*/cache.db`, `~/.local/share/evolution/calendar/*/calendar.ics`)
  et fichiers ou adresses `.ics` listés dans `<dossier des données>/calendars.txt` ; récurrences,
  exceptions et annulations développées ; `me` d'après les comptes en ligne de GNOME ou
  `STENO_EMAIL`.
- **`retranscribe`** : Canary 180M Flash, langues `fr`, `en`, `es`, `de` ; autre langue →
  `model_missing` récupérable (le direct est gardé).
- Annulation d'écho WebRTC sur le micro (sauf `room`), référence = son capté de l'app ; active
  seulement quand la sortie par défaut n'est pas un casque (`STENO_AEC=0|1` pour forcer).
- `start.keep_audio` : si `true` (diarisation ou rétention > 0), le sidecar écrit
  `them.wav` (et `me.wav`) dans `audio_dir` ; sinon aucun fichier audio n'est écrit.
- `diarize` : le sidecar lit `them.wav` et répond par `diarization` (ou `error`).
  Le cœur attribue ensuite `Eux-1`, `Eux-2`… aux segments `them` selon le chevauchement.
- `retranscribe` (fin de réunion, avant la diarisation, si `audio.final_transcription` ≠ `off`) :
  chaque phrase est retranscrite depuis `me.wav` / `them.wav` avec Canary 1B v2, langue imposée.
  `retranscription` ne liste que les phrases dont le nouveau texte est exploitable (non vide,
  sans boucle de répétition) ; les autres gardent le texte du direct. Le cœur remplace le texte
  (`segments.text`, index plein texte compris) et émet `steno://segment` pour chacune.
- Sans `start`, le sidecar ne capture rien mais continue de surveiller les apps et émet
  `call`. Le cœur n'en déduit **jamais** un démarrage : il met à jour `LiveState.call` et,
  si `audio.auto_detect_calls`, envoie une notification (« Appel Slack détecté… »).
- La fin de l'appel (`call` avec `active: false`) n'arrête pas l'écoute.

Simulateur : `steno replay <fixture.jsonl> [--speed N] [--exit]` parle exactement ce protocole
(il rejoue un fichier d'exemple). Comme le vrai sidecar, il annonce l'appel (Teams ou autre) tout de
suite mais n'envoie les phrases qu'après `start` ; `stop` met le rejeu en pause. Il permet de
tester l'app et le cœur sans Mac.
The file's `{"type":"calendar","events":[…]}` lines are the simulated calendar: they are not
replayed; `calendar_events` answers with their events that overlap `from`–`to`, and
`permissions.calendar` is then `granted` (`unknown` when the file has none).

---

## 3. Trait `AgentAdapter`

Emplacement : `crates/steno-core/src/agents/`. Aucun autre module ne connaît les noms de
CLI, de modèles ou d'options.

```rust
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> &'static str;            // "claude-code" | "codex" | "antigravity" | "grok" | "gemini" | "opencode" | "custom"
    fn display_name(&self) -> &'static str;
    fn support(&self) -> Support;            // Supported | Experimental | Generic
    async fn detect(&self) -> Detection;     // { installed, version, path, error }
    fn capabilities(&self) -> Capabilities;
    fn efforts(&self) -> Vec<&'static str> { vec![] }   // reasoning levels the CLI accepts (empty = not adjustable)
    fn lists_models(&self) -> bool { false }            // true: list_models asks the CLI; false: static suggestions
    async fn list_models(&self) -> Result<Vec<ModelInfo>> { Ok(vec![]) }   // suggested models; free input stays allowed
    async fn list_mcp(&self) -> Result<Vec<McpServer>>;
    async fn sync_instructions(&self, run_dir: &Path, instructions: &Instructions) -> Result<()>;
    async fn apply_policy(&self, policy: &ReadOnlyPolicy, run_dir: &Path) -> Result<()>;
    async fn run(&self, req: RunRequest, events: mpsc::Sender<AgentEvent>,
                 cancel: CancellationToken) -> Result<RunResult>;
}

pub struct Capabilities {
    pub resume_session: bool,   // reprise d'une session par identifiant
    pub stream_json: bool,      // sortie événementielle exploitable
    pub read_only_files: bool,  // l'agent ne peut pas écrire de fichiers
    pub tool_deny_list: bool,   // sait interdire des outils MCP par motif -> connecteurs activables
    pub extra_dirs: bool,       // peut lire des dossiers hors du dossier courant
}

pub struct RunRequest {
    pub run_dir: PathBuf,            // stable par réunion (reprise de session)
    pub prompt: String,
    pub tier: Tier,                  // Quick | Search | Summary
    pub model: Option<String>,       // résolu depuis les réglages (vide = défaut du CLI)
    pub effort: Option<String>,      // reasoning level ("low", "high"…), None = CLI default
    pub session: SessionMode,        // None | New(String) | Resume(String)
    pub repos: Vec<PathBuf>,         // dépôts lisibles pour cette exécution
    pub tools: ToolAccess,           // None | ReadOnly { connectors: Vec<String> }
    pub max_tool_calls: u32,
    pub timeout: Duration,
    pub allow_api_key: bool,         // allow API keys found in the environment (off by default)
    pub run_id: String,              // unique run id (temporary files)
    pub hook_exe: Option<PathBuf>,   // executable that answers `policy-hook <file>`
    pub skills: bool,                // let the agent use the user's skills (searches only)
}

pub struct ModelInfo { pub id: String, pub name: String, pub note: Option<String> }

pub enum AgentEvent { Started { session_id: Option<String> }, ToolCall { name: String },
                      Text(String), Stderr(String) }

pub struct RunResult { pub answer: String, pub session_id: Option<String>,
                       pub tools_used: Vec<String>, pub model: Option<String>,
                       pub denied_tools: Vec<String> }
```

Règle de sécurité : si `capabilities().tool_deny_list == false`, les connecteurs de cet
adaptateur sont enregistrés avec `write_blockable = 0` et ne peuvent pas être activés ; le
cœur refuse aussi de les passer dans `ToolAccess::ReadOnly`.

---

## 4. Réglages

Table `settings` : une ligne par section, `value` = JSON. Les sections absentes prennent
leurs valeurs par défaut. Structure (valeurs par défaut) :

```json
{
  "agent": {
    "provider": "claude-code",
    "tiers": {
      "search":  { "agent": "", "model": "", "effort": "" },
      "summary": { "agent": "", "model": "", "effort": "" },
      "quick":   { "agent": "", "model": "", "effort": "" }
    },
    "models": { "quick": "", "search": "", "summary": "" },
    "custom_command": "",
    "allow_api_key": false,
    "max_parallel": 2,
    "timeouts_s": { "quick": 90, "search": 240, "summary": 300 },
    "live_tool_budget": 15,
    "live_context_seconds": 120
  },
  "voice": {
    "wake_word": "Sténo",
    "strict": false,
    "prefixes": ["ok", "dis", "hey"],
    "variants": [],
    "verbs": {
      "search": ["cherche", "recherche", "regarde", "trouve"],
      "check": ["vérifie", "confirme"],
      "note": ["note", "ajoute une action", "retiens"],
      "recap": ["résume", "récap", "récapitule"],
      "cancel": ["annule"],
      "stop": ["arrête l'écoute", "arrête d'écouter", "arrête la transcription", "termine la réunion", "fin de la réunion"]
    },
    "feedback": "system",
    "feedback_device": null
  },
  "audio": {
    "auto_detect_calls": true,
    "input_device": null,
    "input_device_name": null,
    "call_apps": [
      { "id": "teams", "name": "Microsoft Teams", "bundles": ["com.microsoft.teams2", "com.microsoft.teams"], "enabled": true, "browser": false },
      { "id": "slack", "name": "Slack", "bundles": ["com.tinyspeck.slackmacgap"], "enabled": true, "browser": false },
      { "id": "zoom", "name": "Zoom", "bundles": ["us.zoom.xos", "us.zoom.CptHost"], "enabled": true, "browser": false },
      { "id": "webex", "name": "Webex", "bundles": ["Cisco-Systems.Spark", "com.cisco.webexmeetingsapp", "com.webex.meetingmanager"], "enabled": true, "browser": false },
      { "id": "discord", "name": "Discord", "bundles": ["com.hnc.Discord"], "enabled": true, "browser": false },
      { "id": "facetime", "name": "FaceTime", "bundles": ["com.apple.FaceTime"], "enabled": true, "browser": false },
      { "id": "browser", "name": "Navigateur (Google Meet…)", "bundles": ["com.google.Chrome", "com.apple.Safari", "company.thebrowser.Browser", "com.microsoft.edgemac", "org.mozilla.firefox", "com.brave.Browser"], "enabled": false, "browser": true }
    ],
    "silence_ms": 1000,
    "vocabulary": [],
    "end_grace_s": 60,
    "shortcuts": { "toggle": "Ctrl+Alt+S", "aside": "Ctrl+Alt+Space", "quick_note": "Ctrl+Alt+N" },
    "final_transcription": "fr"   // langue de la retranscription de fin de réunion, ou "off"
  },
  "correlation": { "mode": "on_demand", "budget": 30, "exclude_words": ["1:1", "RH", "entretien", "perso"] },
  "privacy": { "audio_retention_days": 0, "transcript_retention_days": null,
               "auto_export_dir": null, "excluded_titles": [] },
  "profile": { "name": "", "role": "", "team": "", "jira_key_pattern": "[A-Z][A-Z0-9]+-\\d+",
               "theme": "auto" },  // auto | light | dark (thème natif de toutes les fenêtres)
  "report": { "default_template": "standard",   // format utilisé quand la réunion n'en choisit pas
              "ask_context_on_stop": false,      // fenêtre de contexte à l'arrêt (sinon arrêt en un clic)
              "context_delay_s": 60,             // délai avant la rédaction pour ajouter du contexte (0 = immédiat)
              "templates": [ { "id": "perso-note-comex", "name": "Note COMEX", "instructions": "…" } ] },
  "updates": { "check": true,           // vérifier les nouvelles versions (démarrage puis toutes les 6 h)
               "auto_install": false }  // installer automatiquement hors réunion (redémarrage manuel)
}
```

- `agent.provider` : agent par défaut (`claude-code`, `codex`, `antigravity`, `grok`, `gemini`,
  `opencode`, `custom`).
- `agent.tiers` : choix **par usage**. `search` = recherches en direct, questions sur
  l'historique et corrélation ; `summary` = compte rendu de fin de réunion ; `quick` = notes,
  récapitulatifs, tests. Pour chacun : `agent` (vide = `provider`), `model` (vide = modèle par
  défaut du CLI ; texte libre accepté) et `effort` (niveau de réflexion, vide = défaut du CLI ;
  valeurs proposées par l'agent, voir `AgentInfo.efforts`). `agent.models` est l'ancien format
  (modèle par niveau pour l'agent par défaut), encore lu si `tiers.*.model` est vide.
- `voice.feedback` : `none | system | headset`. `voice.feedback_device` : UID de la sortie où
  jouer le signal (`null` = sortie par défaut ; `headset` sans périphérique choisi = sortie par
  défaut si c'est un casque, sinon rien).
- `audio.input_device` : UID du micro choisi (`null` = micro par défaut du système) ;
  `audio.input_device_name` : son nom au moment du choix (affiché s'il est débranché).
- `audio.auto_detect_calls` (ancien nom accepté : `auto_detect_teams`) : proposer
  (notification) de démarrer l'écoute quand un appel commence dans une app surveillée.
- `audio.call_apps` : apps de visio surveillées. L'utilisateur peut en ajouter (identifiant
  libre en minuscules, nom, préfixes de bundle). Valeurs par défaut propres à la plateforme :
  sous Linux, noms d'exécutables et identifiants Flatpak (`teams-for-linux`, `slack`,
  `com.slack.Slack`, `zoom`, `us.zoom.Zoom`, `webex`, `discord`, `chrome`, `firefox`…), sans
  FaceTime ; des réglages importés d'un Mac reçoivent ces valeurs en plus des leurs. `browser: true` signale qu'on capte **tout**
  le son du navigateur (autres onglets compris) : désactivé par défaut, avec avertissement.
  `audio.end_grace_s` : obsolète, ignoré.
- `correlation.mode` : `off | on_demand | auto`.
- Import/export TOML : mêmes sections, **sans** `profile.name`, `privacy.auto_export_dir`,
  ni dépôts (aucun chemin, aucune donnée personnelle).

---

## 5. API de l'app (Tauri 2)

Le front appelle `invoke(nom, args)` ; les noms d'arguments sont en `camelCase` côté JS
(conversion automatique de Tauri). Erreur : chaîne en français.

### Types (TypeScript)

```ts
type Channel = "me" | "them";
type Meeting = { id: string; title: string | null; title_source: string | null;
  started_at: string; ended_at: string | null;
  status: "planned" | "live" | "ended" | "processing" | "done" | "error"; /* planned: scheduled, started_at = scheduled time */
  confidential: boolean; agent_provider: string | null; correlation: "off" | "on_demand" | "auto";
  call_app: string | null /* identifiant de l'app de l'appel (teams, slack…), null si démarrage sans appel */ };
type MeetingSummary = Meeting & { correlated: boolean; open_actions: number;
  segment_count: number; snippet: string | null };
type Segment = { id: number; meeting_id: string; seq: number; ts: string; t0: number | null; t1: number | null;
  channel: Channel; speaker: string; text: string; private: boolean; is_echo: boolean; request_id: string | null };
type Speaker = { label: string; display_name: string | null };
type Note = { id: number; meeting_id: string; ts: string; text: string; source: "user" | "voice" };
type Request = { id: string; meeting_id: string | null; created_at: string;
  origin: "voice" | "text" | "hotkey" | "cli"; intent: "search" | "check" | "note" | "recap" | "cancel" | "stop" | "history";
  query: string; context_seconds: number | null; tier: "quick" | "search" | "summary";
  status: "queued" | "running" | "done" | "error" | "cancelled" | "refused";
  started_at: string | null; finished_at: string | null; error: string | null;
  summary: string | null; answer_md: string | null; tools_used: string[] };
type Report = { meeting_id: string; markdown: string; generated_at: string; provider: string | null;
  model: string | null; correlated: boolean; version: number };
type ReportVersion = { version: number; generated_at: string; correlated: boolean };
type ReportRef = { id: number; kind: string; ref: string; title: string | null; url: string | null;
  verified: boolean; note: string | null };
type Decision = { id: number; meeting_id: string; text: string; created_at: string };
type Action = { id: number; meeting_id: string; owner: string | null; text: string; due: string | null;
  status: "open" | "done"; meeting_title?: string | null; meeting_started_at?: string };
type Draft = { id: number; kind: string; target: string | null; body: string; created_at: string };
type MeetingDetail = { meeting: Meeting; speakers: Speaker[]; notes: Note[]; requests: Request[];
  report: Report | null; report_versions: ReportVersion[]; refs: ReportRef[];
  decisions: Decision[]; actions: Action[]; drafts: Draft[] };
type SearchHit = { meeting_id: string; title: string | null; started_at: string;
  kind: "segment" | "report"; snippet: string; ts: string | null };
type LiveState = { state: "idle" | "listening" | "paused" | "processing";
  meeting: Meeting | null; levels: { me: number; them: number }; aside: boolean;
  mic_muted: boolean;       /* mic muted in Sténo (`set_mic_muted`): nothing you say is transcribed */
  sidecar: "running" | "starting" | "stopped" | "missing" | "error"; last_error: string | null;
  pending_requests: number;
  call: { active: boolean; app: string | null; app_name: string | null; title: string | null } };
type AgentInfo = { id: string; name: string; support: "supported" | "experimental" | "generic";
  installed: boolean; version: string | null; error: string | null;
  capabilities: { resume_session: boolean; stream_json: boolean; read_only_files: boolean;
                  tool_deny_list: boolean; extra_dirs: boolean };
  efforts: string[];        /* niveaux de réflexion acceptés par le CLI (ex. low…max) ; vide = non réglable */
  lists_models: boolean };  /* true : `list_models` interroge le CLI ; false : suggestions + saisie libre */
type ModelInfo = { id: string; name: string; note: string | null };
type AudioDevice = { id: string; name: string; default: boolean;
  transport: "builtin" | "usb" | "bluetooth" | "virtual" | "aggregate" | "other" };
type Connector = { id: number; provider: string; name: string; status: string; detected_at: string;
  use_live: boolean; use_correlation: boolean; write_blockable: boolean };
type Repo = { id: number; label: string; path: string; use_live: boolean; use_correlation: boolean;
  exclude_globs: string };
type WakeCheck = { warnings: string[]; errors: string[]; syllables: number;
  history_count: number; meetings_scanned: number };
type VariantCheck = { accepted: string[]; rejected: { variant: string; reason: string }[] };
type WakeDetect = { detected: boolean; intent: string | null; query: string | null };
type DoctorItem = { name: string; status: "ok" | "warn" | "error"; message: string; fix: string | null };
type Settings = /* section 4 */;
```

### Commandes

| Commande | Arguments | Retour |
|---|---|---|
| `live_state` | — | `LiveState` |
| `start_meeting` | `title?: string, inPerson?: boolean` (sans titre : titre de l'appel en cours s'il y en a un ; le son capté est celui de cette app. `inPerson` : réunion en présentiel, micro du Mac seul, `call_app = "room"`, phrases « Salle » puis intervenants identifiés en fin de réunion sur `me.wav`) | `Meeting` |
| `stop_meeting` | — | `null` |
| `set_aside` | `on: boolean` | `null` |
| `set_live_confidential` | `on: boolean` | `null` |
| `ask` | `query: string, meetingId?: string` | `Request` |
| `ask_history` | `query: string` | `Request` |
| `cancel_request` | `id: string` | `null` |
| `recent_requests` | `limit: number` | `Request[]` |
| `list_meetings` | `filter: { query?, from?, to?, correlated?: boolean, openActions?: boolean, limit?, offset? }` | `MeetingSummary[]` |
| `get_meeting` | `id` | `MeetingDetail` |
| `get_transcript` | `id, includePrivate: boolean` | `Segment[]` |
| `get_report_version` | `id, version: number` | `Report` |
| `rename_meeting` | `id, title` | `null` |
| `set_confidential` | `id, on: boolean` | `null` |
| `delete_meeting` | `id` | `null` |
| `rename_speaker` | `meetingId, label, displayName` | `null` |
| `add_note` | `meetingId, text` | `Note` |
| `update_note` | `id: number, text` | `null` |
| `delete_note` | `id: number` | `null` |
| `regenerate_report` | `id` | `null` |
| `get_meeting_context` | `id` | `MeetingContext` `{meeting_id, text, template, files: ContextFile[], report_at}` (sans le texte des documents ; `report_at` : heure UTC de départ de la rédaction si elle attend encore) |
| `set_meeting_context` | `id, text, template?` | `MeetingContext` (`template` : identifiant d'un format, `null` = format par défaut des réglages) |
| `add_context_file` | `id, name, data` (contenu en base64, 20 Mo max) | `ContextFile` `{id, meeting_id, name, kind, size, chars, truncated, added_at}` : texte extrait à l'ajout (PDF, xlsx/xls/ods, csv, docx/odt/rtf/html lus en Rust — `textutil` d'abord sur macOS —, doc via `antiword` ou `catdoc` sous Linux, md/txt/json…), 60 000 caractères gardés |
| `delete_context_file` | `fileId` | `null` (comme toute modification du contexte : repousse d'au moins 30 s une rédaction en attente) |
| `report_now` | `id` | `null` : la rédaction part sans attendre la fin du délai de contexte |
| `get_update_status` | — | `UpdateStatus` `{current, latest, available, url, notes, published_at, checked_at, state, message, source_dir, can_install, log_path}` ; `state` : `idle \| checking \| installing \| installed \| error` |
| `check_update` | — | `UpdateStatus` après lecture de la dernière version publiée (API GitHub publique, sans authentification) |
| `install_update` | — | `null` : lance en arrière-plan la mise à jour des sources (tag `v<version>`, avance rapide sur `main` ou tag détaché) puis `make install` ; refusé pendant une réunion, si les sources ont des modifications ou sont sur une autre branche. Avancement par `steno://update` |
| `restart_app` | — | (app uniquement) redémarre Sténo après une mise à jour ; refusé pendant une réunion |
| `upcoming` | `days?` (défaut 7) | `Upcoming` `{planned: Meeting[], events: UpcomingEvent[], calendar}` : réunions prévues (statut `planned`, `started_at` = heure prévue) et événements du calendrier (macOS, ou sidecar Linux : `{id, title, start, end, all_day, location, url, notes, calendar, attendees: [{name, email, status, organizer, me}], meeting_id}`) ; `calendar` : `granted \| denied \| unknown \| unavailable` (sous Linux, `unavailable` tant que le sidecar n'annonce pas `granted`) |
| `plan_meeting` | `title, scheduledAt` (ISO 8601), `eventId?` | `Meeting` prévue ; depuis un événement : invités (sauf soi et les refus) en participants, description de l'invitation en contexte. Démarrer l'écoute entre 30 min avant et 30 min après l'heure prévue reprend cette réunion |
| `start_planned` | `id, inPerson?` | `Meeting` : démarre l'écoute de la réunion prévue, même en avance |
| `get_participants` | `id` | `Participant[]` `{name, email, source}` (`calendar \| manual`) |
| `set_participants` | `id, participants` | `Participant[]` ; transmis au compte rendu (« Participants invités ») pour nommer les intervenants |
| `prepare_questions` | `id` | `string[]` : 3 à 5 questions de l'agent pour préparer la réunion |
| `prepare_apply` | `id, answers: [{question, answer}]` | `MeetingContext` : l'agent rédige le contexte à partir des réponses (ajouté au contexte existant) |
| `steno_skill_status` | — | `SkillStatus` `{targets: [{agents, path, state: missing\|current\|outdated}], mcp: [{agent, registered}], cli}` : skill « Sténo » et serveur MCP `steno` dans les agents de l'utilisateur |
| `install_steno_skill` | — | `SkillStatus` : copie `SKILL.md` dans `~/.claude/skills/steno` et `~/.agents/skills/steno` (agents présents) et déclare `steno mcp` dans Claude Code et Codex s'il manque |
| `template_from_example` | `example` | `TemplateDraft` `{name, instructions}` : format déduit par l'agent (niveau `quick`, sans outil) d'un compte rendu collé ; rien n'est enregistré |
| `list_report_templates` | — | `Template[]` `{id, name, description, instructions, builtin}` : formats intégrés (`standard`, `daily`, `client`, `atelier`, `arbitrage`, `mail`, `slack`) puis formats perso (`report.templates`) |
| `correlate` | `id` | `null` |
| `export_meeting` | `id, format?: "markdown" \| "txt" \| "srt" \| "vtt"` | `string` (markdown complet par défaut ; `txt`/`srt`/`vtt` : transcription seule) |
| `save_export` | `id, format` | `string \| null` (app : fenêtre « Enregistrer sous… » puis écriture ; null si annulé ou hors app) |
| `print_window` | — | `null` (app : impression de la fenêtre, donc PDF ; hors app : `window.print()` côté interface) |
| `open_main` | `route?: string` | `null` (app: shows the main window with the Dock icon and opens `route` in it; called by the menu bar panel; outside the app the UI navigates by itself) |
| `search` | `query` | `SearchHit[]` |
| `list_actions` | `filter: { status?, owner?, from?, to? }` | `Action[]` |
| `set_action_status` | `id: number, status` | `null` |
| `list_decisions` | `since?: string` | `Decision[]` |
| `get_settings` | — | `Settings` |
| `save_settings` | `settings: Settings` | `Settings` |
| `export_settings` | — | `string` (TOML) |
| `import_settings` | `toml: string` | `Settings` |
| `detect_agents` | — | `AgentInfo[]` |
| `list_audio_devices` | — | `{ inputs: AudioDevice[]; outputs: AudioDevice[] }` (dernière liste reçue du sidecar ; vide sans sidecar) |
| `list_models` | `provider: string` | `ModelInfo[]` (modèles proposés par le CLI ; la saisie libre reste possible) |
| `test_agent` | `provider: string`, `model?: string`, `effort?: string` | `{ ok: boolean, message: string, duration_ms: number }` |
| `list_connectors` | — | `Connector[]` |
| `refresh_connectors` | — | `Connector[]` |
| `update_connector` | `id: number, useLive: boolean, useCorrelation: boolean` | `Connector` |
| `list_repos` | — | `Repo[]` |
| `add_repo` | `path, label?` | `Repo` |
| `update_repo` | `repo: Repo` | `Repo` |
| `remove_repo` | `id: number` | `null` |
| `pick_folder` | — | `string \| null` |
| `check_wake_word` | `word, strict: boolean` | `WakeCheck` |
| `generate_variants` | `word` | `VariantCheck` |
| `check_variants` | `word, variants: string[]` | `VariantCheck` |
| `wake_detect` | `text` | `WakeDetect` |
| `wake_test_record` | `seconds: number` | `{ text: string }` |
| `doctor` | — | `DoctorItem[]` |
| `delete_all_data` | — | `null` |
| `log_ui_error` | `message` | `null` (consigné dans logs/steno.log) |
| `get_permissions` | — | `{ microphone, system_audio, accessibility, calendar: "granted" \| "denied" \| "unknown", available: boolean, platform, sound_server, session, global_shortcuts, app_command }` (redemande l'état au sidecar ; `available` = sidecar lancé ; champs système, facultatifs : `platform` `macos \| linux`, `sound_server` (« pipewire … », « pulseaudio 16.1 »…), `session` `x11 \| wayland`, `global_shortcuts` (raccourcis globaux enregistrés), `app_command` (commande de l'app pour les raccourcis du bureau, clé runtime `app_shortcuts`)) |
| `request_permission` | `kind: "microphone" \| "system_audio" \| "accessibility" \| "calendar"` | `null` (invite macOS affichée par le sidecar ; sans effet si déjà répondu) |
| `open_permission_settings` | `kind` | `null` (ouvre la page des Réglages Système du droit ; adresses fixes. Linux : réglages du son — `gnome-control-center sound`, sinon `pavucontrol` — pour `microphone` et `system_audio`, erreur pour les autres) |
| `complete_onboarding` | — | `null` (assistant terminé ou passé : il ne se rouvre plus) |
| `pause_meeting` | — | `null` (écoute en pause : la réunion continue, rien n'est transcrit ; `LiveState.state = "paused"`) |
| `resume_meeting` | — | `null` (reprise de l'écoute) |
| `set_mic_muted` | `on: boolean` | `null` (micro coupé dans Sténo : rien de ce que vous dites n'est transcrit ; rétabli à la fin de la réunion ; `LiveState.mic_muted`) |
| `delete_segment` | `id: number` | `null` (phrase jamais dite retirée ; événement `steno://segment_deleted`) |

### Événements (`listen`)

| Événement | Charge |
|---|---|
| `steno://live` | `LiveState` (à chaque changement d'état, niveaux au plus 4×/s) |
| `steno://segment` | `Segment` |
| `steno://segment_deleted` | `{ meeting_id, id }` (phrase retirée) |
| `steno://request` | `Request` (création et chaque changement de statut) |
| `steno://report` | `{ meeting_id: string, version: number }` |
| `steno://meetings` | `null` (la liste a changé) |
| `steno://update` | `UpdateStatus` (nouvelle version, installation en cours, terminée ou en échec) |
| `steno://navigate` | `{ route: string }` (ex. `"/meetings/<id>?tab=requests&request=<rid>"`) |
| `steno://settings` | `Settings` (settings saved: every window and the native menu follow the language, wake word and shortcuts) |

---

## 6. Fichiers d'exemple

- `fixtures/daily-45min.jsonl` : réunion synthétique au format du protocole du sidecar
  (§2, messages sidecar → cœur), 45 min, en français. Contient : des tickets `APP-…`, le
  service `ms-booking`, une décision, deux actions, une commande vocale (« Sténo, cherche… »),
  un aparté (`private: true`), des mots pièges proches de « Sténo » (« scénario », « stylo », « sténographie »),
  un écho (même phrase sur `them` puis `me` à moins de 2 s).
- `fixtures/expected.json` : ce que les tests attendent de ce fichier (nombre de commandes,
  intentions, phrase privée…).
- `fixtures/fake-bin/` (`claude`, `codex`, `agy`, `grok`): fake agent CLIs (scripts) that
  mimic the real CLIs' output, so the adapters are tested without a subscription. The tests
  put this folder first in `PATH`.

---

## 7. Backend HTTP (`steno serve`)

`steno serve --port 4180 --ui app/ui/dist` expose l'API du §5 sur `127.0.0.1` et sert
l'interface sur la même origine. Ouvrir `http://127.0.0.1:4180/?backend=http`.

- `POST /api/invoke/<commande>` : corps = arguments JSON en camelCase (vide = aucun) ;
  200 → JSON du résultat (`null` si rien) ; 400 → `{"error": "…"}` ; 404 si commande inconnue.
- `GET /api/events` : flux SSE ; `event:` = nom de l'événement du §5, `data:` = charge JSON.
- Variables : `STENO_HOME`, `STENO_SIDECAR` (ex. `steno replay fichier.jsonl --speed 20`).
- `pick_folder` renvoie `null` (pas de sélecteur natif).

Le répartiteur (`steno_core::api::dispatch`) couvre exactement les commandes du §5 ; un test
vérifie que chaque commande listée ici est connue.

---

## 8. App de bureau : actions en ligne de commande

`steno-app --toggle` (démarrer / arrêter l'écoute), `--aside` (aparté, en bascule), `--note`
(note rapide), `--show` (ouvrir la fenêtre). Une seconde instance transmet l'action à l'app déjà
lancée (`tauri-plugin-single-instance`) puis se termine ; si l'app ne tourne pas, elle démarre et
exécute l'action. Sert aux raccourcis personnalisés du bureau sous Wayland, où les raccourcis
globaux de Tauri ne fonctionnent pas, et aux actions du raccourci de bureau
(`packaging/linux/steno.desktop`).
