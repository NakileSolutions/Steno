# steno-audio — sidecar audio de Sténo

Binaire Swift lancé par le cœur de Sténo. Il ne fait que de l'audio vers du texte :

- capture du **micro** (canal `me`) avec `AVAudioEngine` et voice processing (annulation d'écho) ;
- capture du **son des apps de visio** (canal `them`) par un *process tap* Core Audio sur tout
  l'arbre de processus des apps captées : Teams, Slack, Zoom, navigateur… (macOS 14.2+) ;
- rééchantillonnage en 16 kHz mono, **VAD** (Silero via FluidAudio, repli par énergie),
  découpage en énoncés (fin après `silence_ms` de silence ou 25 s), **transcription Parakeet TDT v3**
  sur le Neural Engine, correction par le vocabulaire ;
- détection des appels dans les apps de visio surveillées (liste fixée par le cœur, commande
  `watch`) et lecture du titre de l'appel (accessibilité) ;
- diarisation hors ligne de `them.wav` à la demande ;
- liste des **périphériques audio** (micros, sorties), choix du micro (au `start` ou pendant la
  capture) et signal sonore discret de détection sur la sortie choisie.

Il dialogue avec le cœur en **JSON lines** sur stdin/stdout, selon `docs/contracts.md` §2.
Les journaux vont **uniquement sur stderr**. Aucune donnée ne quitte le Mac (seul le premier
lancement télécharge les modèles depuis Hugging Face, via FluidAudio).

## Prérequis

- macOS 14.2 ou plus récent, Apple Silicon ;
- Xcode 16 (Swift 5.10 / 6.0) ou plus récent ;
- accès réseau au premier lancement (dépendance FluidAudio et modèles).

## Compilation

```sh
cd audio/StenoAudio
swift build -c release            # binaire : .build/release/steno-audio
swift test                        # tests sans matériel (protocole, vocabulaire, segmenteur…)
.build/release/steno-audio --prefetch-models   # télécharge et compile les modèles (≈ 1 Go)
```

`Package.swift` embarque `Info.plist` dans le binaire (section `__TEXT,__info_plist`) pour que les
descriptions d'usage existent même quand le sidecar est lancé seul.

Variables d'environnement utiles :

| Variable | Effet |
|---|---|
| `STENO_TAP_MODE=global` | plan B : tap sur tout le son du Mac sauf Sténo (si une app sort l'audio par un processus non repéré) |
| `STENO_VAD=energy` | VAD par énergie au lieu de Silero |
| `STENO_MODELS_DIR=<dossier>` | dossier des modèles (sinon le cache par défaut de FluidAudio) |
| `STENO_AUDIO_DEBUG=1` | journaux détaillés sur stderr |

Autres commandes : `--version`, `--help`, `--list-devices` (micros et sorties avec leur UID, tels
que le message `devices` les présente, puis tous les périphériques du HAL avec la raison de leur
exclusion éventuelle), `--list-audio-processes [--apps teams,slack]`
(diagnostic des processus audio vus par Core Audio, avec l'app reconnue pour chacun : Teams seul
par défaut), `--prefetch-models`, et les deux modes spike décrits plus bas. L'option `--apps`
accepte les identifiants du catalogue intégré : `teams`, `slack`, `zoom`, `meet` (navigateurs).

## Apps de visio surveillées

Le cœur fixe la liste par `{"cmd":"watch","apps":[{"id","name","bundles":[préfixes]}]}`, juste
après `ready` puis à chaque changement de réglages. Avant le premier `watch`, seul Teams est
surveillé (`com.microsoft.teams2`, `com.microsoft.teams`). Une liste vide ne surveille plus rien.

- **Appartenance** : un processus appartient à une app si son bundle ID **commence par** l'un de
  ses préfixes (sans casse ; le plus long préfixe l'emporte si deux apps se chevauchent), ce qui
  couvre les auxiliaires (`com.microsoft.teams2.helper…`). Un processus audio sans bundle reconnu
  appartient à l'app dont il descend (PID parent). Sténo lui-même n'est jamais compté.
- **Appel** : toutes les ~2 s, une app est « en appel » dès qu'un de ses processus utilise l'entrée
  audio (`kAudioProcessPropertyIsRunningInput`) ; fin après deux relevés négatifs consécutifs. L'état
  est **séparé par app** : chaque changement émet `{"type":"call","active":…,"app":"<id>","title":…}`
  (plusieurs appels peuvent être actifs en même temps). L'ancien message `teams_call` n'est plus
  émis. Retirer d'un `watch` une app en appel émet sa fin d'appel.
- **Titre** (accessibilité, `CallTitleHeuristics`) : Teams, fenêtre de réunion séparée (heuristique
  d'origine) ; Slack, fenêtre du huddle (« Huddle… », « Appel… » avec le canal), sinon `null` ;
  Zoom, fenêtre « Zoom Meeting » / « Réunion Zoom » (sujet s'il figure dans le titre) ; navigateurs,
  onglet actif « Meet – … » (« Google Meet » si seul le code de réunion est visible) ; autres apps,
  titre de la fenêtre au premier plan, sauf titre générique (nom de l'app, vide). Le titre est figé
  dès qu'un titre plausible est trouvé ; `null` si l'accessibilité est refusée ou si rien ne convient.
- **Capture** : `start.apps` (ex. `["slack"]`) choisit les apps dont le son va sur le canal `them` ;
  absent ou vide = toutes les apps surveillées. Les identifiants inconnus sont ignorés (journal) ;
  s'il n'en reste aucun, toutes les apps surveillées sont captées. Le tap couvre l'union des
  processus de ces apps et il est recréé quand cette liste change (redémarrage, nouvel auxiliaire,
  `watch`). Si aucun processus ne correspond au `start` (app pas encore lancée ou sans audio
  ouvert), le micro démarre quand même, un `error` `tap_failed` récupérable l'explique, et le tap
  est créé dès que les processus apparaissent.

## Périphériques

Code : `AudioDevices.swift` (énumération, filtrage, tri, écouteurs), `FeedbackPlayer.swift`
(signal sonore), `MicCapture.swift` (choix du micro), `CaptureSession` / `Controller` (commandes).

- **Liste** : `{"type":"devices","inputs":[…],"outputs":[…]}`, chaque entrée avec `id` (UID Core
  Audio, `kAudioDevicePropertyDeviceUID`), `name`, `default` et `transport`
  (`builtin | usb | bluetooth | virtual | aggregate | other`, d'après
  `kAudioDevicePropertyTransportType` : Bluetooth et Bluetooth LE → `bluetooth`, agrégats manuels
  et automatiques → `aggregate`, PCI, HDMI, DisplayPort, AirPlay, Thunderbolt, Continuity… →
  `other`). Tri : périphérique par défaut d'abord, puis par nom (sans casse ni accents, nombres
  dans l'ordre naturel), puis par UID.
- **Filtrage** : `inputs` ne garde que les périphériques ayant au moins un canal d'entrée, `outputs`
  au moins un canal de sortie ; sont exclus les entrées dont l'UID ou le nom est vide, les doublons
  d'UID et les agrégats privés : celui du tap de Sténo (`steno-tap-…`, nom `Steno-Tap`) et ceux
  qu'AVAudioEngine crée dans notre processus (`CADefaultDeviceAggregate…`,
  `VPAUAggregateAudioDevice…`). Un agrégat créé par l'utilisateur reste proposé.
- **Émission** : après `ready` (et `permissions`), en réponse à `{"cmd":"devices"}`, et à chaque
  changement de la liste ou des périphériques par défaut (écouteurs HAL sur
  `kAudioHardwarePropertyDevices`, `…DefaultInputDevice`, `…DefaultOutputDevice`), après un
  anti-rebond de 300 ms et seulement si le message change (la création de l'agrégat du tap modifie
  la liste HAL sans changer `devices`).
- **Choix du micro** : `start.input_device` (`null` = micro par défaut du système, en suivant ses
  changements ; champ absent = dernier choix mémorisé, sinon micro par défaut). Le micro choisi est
  réglé sur l'unité du nœud d'entrée d'AVAudioEngine (`kAudioOutputUnitProperty_CurrentDevice`)
  **avant** l'activation du voice processing, puis relu après : s'il a été perdu, il est réglé de
  nouveau (le journal le dit). Si l'UID est introuvable, ou si le micro est débranché pendant la
  capture, le micro par défaut prend le relais et le sidecar émet, une fois par absence,
  `{"type":"error","code":"device_missing","message":"Micro « <uid> » introuvable : micro par défaut utilisé.","recoverable":true}`.
  Dès que le micro réapparaît dans la liste des périphériques, il est repris.
- **`set_input`** : `{"cmd":"set_input","device":"<uid>"|null}` change de micro pendant la capture en
  ne redémarrant que le moteur micro (le tap continue). L'horloge `t0/t1` continue ; le court trou
  est comblé comme lors d'un changement de périphérique (silence dans `me.wav`, horloge du
  segmenteur avancée) et le rééchantillonneur suit la fréquence du nouveau micro. Hors capture, le
  choix est mémorisé pour le prochain `start` (y compris pour `wake_test` hors réunion). `device`
  est obligatoire (`bad_command` sinon).
- **`play`** : `{"cmd":"play","sound":"detect","device":"<uid>"|null}` joue deux notes douces (mi5
  puis la5, 55 + 65 ms ≈ 120 ms, crête ≈ −15 dBFS, fondus de 8 ms) sur la sortie demandée (`null` ou
  sortie introuvable = sortie par défaut). Moteur `AVAudioEngine` séparé, créé à la demande
  (`AVAudioPlayerNode` + buffer PCM synthétisé), sortie réglée par
  `kAudioOutputUnitProperty_CurrentDevice` sur `outputNode`, arrêté après 3 s d'inactivité. Le son
  part de notre processus : il ne passe jamais par l'app d'appel et n'est pas capté par le tap (qui
  ne vise que les processus des apps de visio, ou tout sauf Sténo en mode global). La commande
  rend la main immédiatement. `sound` absent = `detect` ; son inconnu = `bad_command`.

## Intégration dans Steno.app (`externalBin` Tauri)

Tauri attend le binaire suffixé par le triplet de la cible :

```sh
cd audio/StenoAudio
swift build -c release --arch arm64
mkdir -p ../../app/src-tauri/binaries
cp .build/arm64-apple-macosx/release/steno-audio \
   ../../app/src-tauri/binaries/steno-audio-aarch64-apple-darwin
```

Dans `app/src-tauri/tauri.conf.json` :

```json
{ "bundle": { "externalBin": ["binaries/steno-audio"] } }
```

Tauri copie alors le binaire dans `Steno.app/Contents/MacOS/steno-audio` et le signe avec l'app
(signature ad hoc suffisante). Côté Rust, le cœur le lance avec `tauri-plugin-shell`
(`app.shell().sidecar("steno-audio")`) ou `std::process::Command`, écrit les commandes sur stdin
et lit stdout ligne par ligne. Fermer stdin (ou envoyer `{"cmd":"shutdown"}`) arrête le sidecar
proprement.

L'`Info.plist` de **Steno.app** (`app/src-tauri/Info.plist`) doit contenir les mêmes clés que
`audio/StenoAudio/Info.plist` : `NSMicrophoneUsageDescription` et `NSAudioCaptureUsageDescription`.

### Permissions TCC : qui les porte ?

macOS attribue les demandes de permission (micro, enregistrement audio du système) au **processus
responsable**, c'est-à-dire l'app qui a lancé le sidecar. Les permissions sont donc demandées et
accordées **à Steno.app**, avec les textes de son `Info.plist` ; le sidecar en hérite tant qu'il est
lancé par l'app. C'est le processus parent qui en est responsable : le sidecar ne fait que constater
l'état (`permissions`) et signaler les refus (`mic_denied`, `system_audio_denied`).

Conséquences pratiques :

- lancé depuis un terminal (spikes, `cargo tauri dev`), c'est **le terminal** (Terminal, iTerm…) qui
  reçoit les demandes et doit être autorisé ;
- l'accessibilité (titre de l'appel) est accordée à Steno.app dans Réglages Système ›
  Confidentialité et sécurité › Accessibilité ;
- **à vérifier au spike S1** : que l'autorisation accordée à Steno.app couvre bien le sidecar.
  **Plan B** si ce n'est pas le cas : faire la capture en Rust (`objc2-core-audio` pour le tap,
  `objc2-avf-audio` pour le micro) dans le processus principal de l'app, et ne garder dans le sidecar
  que la VAD et la transcription (l'audio lui serait alors envoyé par un tube).

## Spikes à exécuter sur le Mac (phase 0)

### S1 — Capture (bloquant)

But : deux pistes audibles pendant un vrai appel Teams, permissions valables pour le sidecar.
Les étapes 1 à 7 visent Teams ; **S1-bis** les refait avec Slack et Zoom.

1. Compiler : `swift build -c release`.
2. Lancer un appel Teams (réunion avec un collègue, ou Paramètres › Appareils › « Passer un appel
   test »). Parler et faire parler l'autre participant.
3. Repérer les processus audio de Teams :
   `.build/release/steno-audio --list-audio-processes`
   → noter les bundles marqués `teams` dans la colonne « app » et vérifier qu'aucun autre processus (ex.
   `com.apple.WebKit.GPU`, assistant Edge WebView) ne joue le son de l'appel sans être repéré.
4. Depuis le terminal (le terminal doit être autorisé pour le micro et l'audio système) :
   `.build/release/steno-audio --spike-record 60 ~/Desktop/s1`
   → le niveau de « moi » et « eux » s'affiche chaque seconde ; à la fin, ouvrir
   `~/Desktop/s1/me.wav` et `them.wav` dans QuickTime : les deux pistes doivent être audibles,
   séparées, et l'écho de « eux » quasi absent de `me.wav` (AEC).
5. Pendant l'enregistrement, débrancher puis rebrancher le casque : stderr doit indiquer
   « micro reparti » en moins de 5 s.
6. **Critère décisif** : refaire l'étape 4 depuis une **app Tauri vide** qui lance le sidecar avec
   les mêmes arguments (`sidecar("steno-audio").args(["--spike-record","60","/tmp/s1"])`), après
   avoir **retiré** l'autorisation du terminal. Vérifier que les invites système nomment l'app, que
   les deux WAV sont audibles, et que Réglages Système liste l'app (pas `steno-audio`).
7. Si `them.wav` est silencieux alors que Teams joue du son : réessayer avec
   `STENO_TAP_MODE=global` (tap global) ; si ça marche, l'audio passe par un processus non repéré
   (étape 3) → noter son bundle dans `docs/spikes.md`.

### S1-ter — Périphériques : à refaire (changement de micro, AirPods)

But : vérifier la liste des périphériques, le choix du micro, le changement de micro pendant
l'écoute et le repli quand des AirPods sont branchés puis débranchés.

1. `.build/release/steno-audio --list-devices` sans puis avec les AirPods connectés : noter les UID
   du micro intégré et des AirPods ; vérifier `transport` (`builtin`, `bluetooth`), le défaut, et
   qu'aucun agrégat privé n'apparaît dans les deux premières listes. Refaire pendant une capture
   (`--spike-record` dans un autre terminal) : l'agrégat `Steno-Tap` et les agrégats
   d'AVAudioEngine doivent être marqués « agrégat privé » dans la dernière liste (sinon, ajouter leur
   préfixe à `AudioDeviceCatalog.privateAggregateUIDPrefixes`).
2. Mode protocole, commandes envoyées par un tube nommé pour pouvoir agir entre deux commandes :

   ```sh
   mkfifo /tmp/steno-in
   .build/release/steno-audio < /tmp/steno-in > /tmp/steno-out.jsonl &
   exec 3>/tmp/steno-in            # garde le tube ouvert
   tail -f /tmp/steno-out.jsonl    # dans un autre terminal
   echo '{"cmd":"start","keep_audio":true,"audio_dir":"/tmp/s1-ter","input_device":"BuiltInMicrophoneDevice"}' >&3
   # parler 20 s, puis :
   echo '{"cmd":"set_input","device":"<UID des AirPods>"}' >&3
   # parler 20 s dans les AirPods, puis les ranger dans leur boîtier (débranchement), parler 20 s,
   # les ressortir, parler 20 s, puis :
   echo '{"cmd":"set_input","device":null}' >&3
   echo '{"cmd":"play","sound":"detect","device":null}' >&3
   echo '{"cmd":"play","sound":"detect","device":"<UID des AirPods>"}' >&3
   echo '{"cmd":"stop"}' >&3; exec 3>&-
   ```

   Attendu : `devices` juste après `ready` / `permissions`, puis à chaque connexion / déconnexion des
   AirPods (un seul message par changement grâce à l'anti-rebond) ; stderr « micro démarré : « AirPods
   … » » après `set_input` sans redémarrage du tap ; au rangement des AirPods, un `error`
   `device_missing` (une seule fois) et la capture qui continue sur le micro par défaut ; à leur
   retour, « micro « … » de nouveau disponible : repris ». Le journal dit si le second réglage du
   périphérique (après activation du voice processing) a été nécessaire.
3. Ouvrir `/tmp/s1-ter/me.wav` : la parole des quatre périodes est présente, les trous de
   changement de micro durent moins d'une seconde et `me.wav` reste aligné sur `them.wav` (même
   durée à ±0,5 s) ; les segments `me` ont des `t0/t1` continus.
4. `start` avec un UID inexistant (`"input_device":"inexistant"`) → `device_missing` avec ce texte,
   capture sur le micro par défaut. `set_input` hors capture puis `start` sans `input_device` → le
   micro mémorisé est utilisé (journal « capture démarrée (micro « … »…) »).
5. Signal `play` : bref, discret, entendu sur la sortie demandée (AirPods) ou par défaut ; il ne
   doit **pas** être entendu par l'autre participant d'un appel Teams, ni apparaître dans
   `them.wav`. Vérifier aussi qu'il n'est pas atténué par le voice processing du micro.
6. Pendant un appel avec les AirPods comme micro et sortie : AEC toujours actif (`AEC actif` dans le
   journal) et son de l'appel non baissé.

### S1-bis — Slack, Zoom (et Meet) : à refaire

But : vérifier la détection d'appel, le titre et la capture pour chaque app, avec les préfixes de
bundle que le cœur enverra dans `watch`.

1. Pour chaque app (huddle Slack dans un canal, réunion Zoom, Google Meet dans Chrome et Safari) :
   pendant l'appel, `.build/release/steno-audio --list-audio-processes --apps slack,zoom,meet`
   → noter les bundles qui ont `entrée = oui` ou `sortie = oui` et l'app reconnue (colonne « app »).
   Un processus qui joue le son de l'appel avec « — » dans la colonne app est un préfixe manquant
   (ou un processus XPC hors descendance, ex. `com.apple.WebKit.GPU` pour Safari).
2. `.build/release/steno-audio --spike-record 60 ~/Desktop/s1-slack --apps slack` (puis `zoom`,
   `meet`) : la sortie affiche, par app, les processus trouvés, si le micro est utilisé et le
   **titre** lu par l'accessibilité ; `them.wav` doit contenir les participants et rien d'autre.
3. Noter dans `docs/spikes.md` les titres de fenêtre réels (huddle Slack agrandi ou réduit, Zoom en
   français, onglet Meet avec et sans événement d'agenda) pour ajuster `CallTitleHeuristics`.
4. Mode protocole, app pas encore lancée : envoyer `watch` (Slack) puis `start` avec
   `"apps":["slack"]` Slack fermé → `error tap_failed` récupérable ; lancer Slack et un huddle →
   le journal indique « processus de Slack apparus » et `them` reçoit le son.
5. Deux appels simultanés (ex. Teams et un huddle Slack) : deux événements `call` distincts, et la
   fin de l'un n'affecte pas l'autre.

### S2 — Transcription

But : français lisible, noms techniques compris, moins d'une seconde par phrase.

1. `.build/release/steno-audio --prefetch-models` (premier téléchargement des modèles).
2. `.build/release/steno-audio --spike-transcribe ~/Desktop/s1/them.wav`
   puis la même chose sur `me.wav`.
3. Lire la sortie : chaque phrase avec ses bornes et son temps de transcription ; le résumé final
   indique le temps moyen, le maximum et le facteur temps réel. Critère : maximum < 1 s.
4. Noter les noms techniques mal transcrits : ils iront dans le vocabulaire (correction après coup).
5. Facultatif : comparer avec WhisperKit (`whisperkit-cli transcribe --audio-path …`) sur les mêmes
   fichiers et consigner la comparaison dans `docs/spikes.md`.
6. Vérifier les points FluidAudio marqués « À VÉRIFIER (spike S2) » dans `FluidAudioBackend.swift`.

### S4 — Poste de travail (Mac géré par l'entreprise)

But : savoir tout de suite si le MDM bloque une permission.

1. Lister les profils de configuration : `sudo profiles show -type configuration` et chercher une
   charge utile `com.apple.TCC.configuration-profile-policy` (règles `Microphone`,
   `ScreenCapture`/`AudioCapture`, `Accessibility`).
2. Réglages Système › Confidentialité et sécurité : vérifier que Micro, « Enregistrement audio du
   système » (ou « Enregistrement de l'écran et audio du système ») et Accessibilité sont
   modifiables (pas grisés).
3. Lancer `printf '{"cmd":"permissions"}\n' | .build/release/steno-audio` : on doit voir `ready`
   puis deux messages `permissions` (micro `granted`, accessibilité `granted` une fois autorisée).
4. Refaire S1 étape 4. Si la création du tap échoue, la sortie indique « accès à l'audio système
   refusé » (protocole : `system_audio_denied`). **Si une permission est bloquée : arrêter et
   prévenir l'utilisateur** (PLAN §13).

### Essai du protocole à la main

```sh
( echo '{"cmd":"watch","apps":[{"id":"teams","name":"Microsoft Teams","bundles":["com.microsoft.teams2","com.microsoft.teams"]},{"id":"slack","name":"Slack","bundles":["com.tinyspeck.slackmacgap"]}]}'
  echo '{"cmd":"start","vocabulary":["Sténo","ms-booking"],"silence_ms":1000,"keep_audio":true,"audio_dir":"/tmp/steno-essai"}'
  sleep 30
  echo '{"cmd":"aside","on":true}'; sleep 5; echo '{"cmd":"aside","on":false}'
  sleep 5
  echo '{"cmd":"stop"}'
  sleep 3
  echo '{"cmd":"diarize","meeting_id":"essai","audio_dir":"/tmp/steno-essai"}'
  sleep 60
) | .build/release/steno-audio
```

Attendu : `ready`, `permissions`, `devices`, `call` à chaque début ou fin d'appel Teams ou Slack,
`state listening`, des `level` (4 par seconde au plus), des
`segment` (ceux dits pendant l'aparté avec `"private":true`), `state stopped`, puis `diarization`.
La fin de stdin arrête le sidecar.

## Organisation du code

| Fichier | Rôle |
|---|---|
| `Sources/StenoAudio/main.swift` | point d'entrée (appelle `StenoAudioMain.run`) |
| `StenoAudioMain.swift` | arguments, thread de lecture de stdin, signaux, boucle de commandes |
| `Protocol.swift` | messages des deux sens (Codable), apps surveillées (`WatchedApp`), choix de périphérique (`DeviceSelection`), erreurs `bad_command`, horodatage ISO 8601 |
| `Output.swift` | sortie stdout sérialisée (une ligne par message), journaux stderr, utilitaires |
| `Controller.swift` | traitement des commandes, cycle de vie de la capture |
| `CaptureSession.swift` | micro + tap + deux canaux, recréation du tap (processus apparus ou modifiés), aparté, niveaux |
| `ChannelPipeline.swift` | par canal : trous du flux, WAV, VAD, segmenteur, ASR, vocabulaire |
| `CallMonitor.swift` | apps surveillées (toutes les 2 s) : processus par app, appel par app avec anti-rebond, lecture des fenêtres (accessibilité) |
| `CallTitleHeuristics.swift` | heuristiques pures de titre d'appel (Teams, Slack, Zoom, Meet, par défaut) |
| `ProcessTap.swift` | process tap Core Audio + agrégat privé + IOProc |
| `MicCapture.swift` | micro `AVAudioEngine` avec AEC, choix du micro, repli et reprise, redémarrage sur changement de périphérique |
| `AudioDevices.swift` | périphériques : lecture HAL, classement du transport, filtrage, tri (fonctions pures), écouteurs avec anti-rebond, réglage du périphérique d'un nœud AVAudioEngine |
| `FeedbackPlayer.swift` | signal sonore `play` : synthèse (pure) et moteur de lecture séparé |
| `Resampler.swift` | mixage mono, `AVAudioConverter` vers 16 kHz |
| `Segmenter.swift` | protocole VAD, VAD énergie, segmenteur d'énoncés |
| `Transcriber.swift` | protocoles `SpeechRecognizer` / `SpeechBackend`, chargement des modèles |
| `VocabularyCorrector.swift` | correction par dictionnaire (casse, accents, séparateurs) |
| `FluidAudioBackend.swift` | **seul fichier dépendant de FluidAudio** (Parakeet, Silero, diarisation) |
| `Diarizer.swift` | mise en forme des tours de parole (`S1`, `S2`…) |
| `WavWriter.swift` | WAV 16 bits 16 kHz mono (seulement si `keep_audio`), fichiers en 0600 |
| `Permissions.swift` | état micro / audio système / accessibilité |
| `Levels.swift` | niveaux RMS en dBFS, émis au plus 4 fois par seconde |
| `CoreAudioUtils.swift` | accès aux propriétés du HAL, écouteurs de propriétés |
| `Spikes.swift` | modes `--spike-record`, `--spike-transcribe`, diagnostics (`--list-audio-processes`, `--list-devices`), préchargement |

Choix notables :

- `ts` d'un segment = heure locale (avec fuseau) de la **fin** de l'énoncé ; `t0`/`t1` = secondes
  depuis `start`, sur une horloge monotone. Quand le tap disparaît (Teams redémarre, veille), le
  trou est détecté et comblé (silence dans le WAV, horloge avancée) : `t0`/`t1` et `them.wav`
  restent alignés sur le `start`, ce qui permet au cœur de croiser diarisation et segments.
- Un énoncé `me` est `private` si l'aparté a été actif à un moment quelconque pendant l'énoncé.
- Fin d'appel déclarée, app par app, après deux relevés négatifs consécutifs (≈ 4 s) ; le délai
  de grâce de 60 s reste du ressort du cœur.
- Le titre de l'appel est figé dès qu'un titre plausible est trouvé pendant l'appel ; un titre
  générique ou douteux donne `null` plutôt qu'un titre faux (ex. Slack sans fenêtre de huddle).
- `watch` : entrées sans `id` ou sans préfixe de bundle ignorées, doublons d'`id` ignorés (le
  premier gagne), `name` absent = `id`. Un champ de mauvais type donne `bad_command`.
- Le catalogue intégré (`teams`, `slack`, `zoom`, `meet`) ne sert qu'aux modes en ligne de
  commande ; en mode protocole, seule la liste `watch` du cœur compte.
- Codes d'erreur de `diarize` : `bad_command` (them.wav absent), `model_missing` (modèles de
  diarisation indisponibles), `asr_failed` (échec du traitement).
- `stop` attend la transcription des énoncés en cours (au plus 15 s) avant d'émettre
  `state stopped`.
- Périphériques : `start.input_device` absent ≠ `null` (absent = dernier choix mémorisé). Le choix
  du micro survit d'une capture à l'autre. `device_missing` n'est émis qu'une fois par absence du
  micro (pas de limiteur de 30 s, contrairement aux autres erreurs de capture). Le classement du
  transport recopie les codes `kAudioDeviceTransportType…` pour rester une fonction pure testable.

## Points À VÉRIFIER sur le Mac

Le code a été écrit sans toolchain Swift ni SDK macOS : il doit être compilé et éprouvé sur le Mac.
Points incertains, par ordre de priorité :

1. **Héritage TCC** par le sidecar lancé depuis Steno.app (S1, étape 6) ; sinon plan B Rust.
2. **API FluidAudio** (relevée sur la v0.18.4, détails en tête de `FluidAudioBackend.swift`) :
   `AsrModels.downloadAndLoad(to:version:)`, `AsrManager.transcribe(_:decoderState:language:)`
   (le README de FluidAudio montre encore `transcribe(samples)`), `language: .french`,
   `VadManager.processStreamingChunk`, `OfflineDiarizerManager.prepareModels()/process(_:)`.
3. **Processus audio des apps** (S1, S1-bis) : bundles réels de Teams (`com.microsoft.teams2*` ?),
   Slack (`com.tinyspeck.slackmacgap*`), Zoom (`us.zoom.xos`, `us.zoom.CptHost` ? — préfixes du
   catalogue intégré) et des navigateurs ; existence d'un processus de rendu hors préfixe et hors
   descendance (ex. XPC `com.apple.WebKit.GPU` pour Safari, non couvert : Meet dans Safari ne sera
   probablement ni détecté ni capté) → `--list-audio-processes --apps …`.
4. **`CATapDescription`** : init `stereoMixdownOfProcesses:` avec `[AudioObjectID]`, propriétés
   `name`, `isPrivate`, `muteBehavior`, init `stereoGlobalTapButExcludeProcesses:` (plan B).
5. **Refus de l'audio système** : code OSStatus renvoyé (hypothèse `'nope'` ou `'!hog'`), ou tap
   créé normalement mais silencieux (alors l'état reste `unknown` ; `granted` dès que de l'audio non
   nul arrive).
6. **Voice processing** : canal 0 = voix traitée ; réglage
   `voiceProcessingOtherAudioDuckingConfiguration` (le son de Teams ne doit pas baisser) ; besoin de
   `mainMixerNode` ; repli sans AEC si le démarrage échoue.
7. **Boucle de reconfiguration** : vérifier que la création de l'agrégat privé du tap ne déclenche
   pas `AVAudioEngineConfigurationChange` sur le moteur micro (redémarrages en boucle).
8. **Titres de fenêtre** réels (`CallTitleHeuristics`) : nouveau Teams en français ; fenêtre du
   huddle Slack (titre exact, existe-t-elle quand le huddle est réduit ?) ; Zoom en français
   (« Réunion Zoom » ?) ; onglet Meet (« Meet – <événement> », séparateur exact) ; attributs `AXMain`
   et `AXFocusedWindow` (comparaison par `CFEqual`).
9. `proc_pidinfo` / `PROC_PIDTBSDINFO` accessibles via `import Darwin` (descendance des processus).
10. Réception de `NSWorkspace.didWakeNotification` dans un binaire sans `NSApplication`
    (boucle `RunLoop.main.run()`).
11. Option d'édition de liens `-sectcreate __TEXT __info_plist` (`unsafeFlags`) acceptée par
    `swift build` pour le paquet racine.
12. Latence réelle fin de phrase → segment (< 3 s visé) avec `silence_ms = 1000` et des trames VAD
    de 256 ms.
13. **`NSWorkspace.shared.runningApplications`** lu sur la file du moniteur (hors fil principal)
    dans un binaire sans `NSApplication` : liste à jour ? (sinon revenir à
    `NSRunningApplication.runningApplications(withBundleIdentifier:)` par préfixe connu). Filtre
    `activationPolicy != .prohibited` pour ne pas interroger l'accessibilité des auxiliaires.
14. **Tap multi-apps** : `CATapDescription(stereoMixdownOfProcesses:)` avec les processus de
    plusieurs apps (union) ; recréation sans clic audible quand un auxiliaire apparaît.
15. **Faux positifs de détection** : un navigateur surveillé qui utilise le micro hors Meet
    déclenche un `call` ; Slack peut ouvrir le micro hors huddle (test du micro dans les
    préférences).
16. **Micro choisi et voice processing** (S1-ter) : `kAudioOutputUnitProperty_CurrentDevice` sur
    l'`AudioUnit` de `inputNode` réglé avant `setVoiceProcessingEnabled(true)` est-il conservé, ou
    faut-il le second réglage (après) ? Avec VoiceProcessingIO, la propriété vise-t-elle bien
    l'entrée (et pas la sortie) ? Le voice processing accepte-t-il un micro autre que celui par
    défaut (sinon : repli sans AEC, visible dans le journal) ?
17. **Sortie du signal sonore** : `kAudioOutputUnitProperty_CurrentDevice` sur `outputNode` d'un
    moteur sans entrée, réglé avant la connexion du mixeur ; mono centré sur les deux canaux ;
    signal non atténué par le « ducking » du voice processing du micro.
18. **Agrégats privés visibles** dans `kAudioHardwarePropertyDevices` de notre processus : préfixes
    réels (`steno-tap-`, `CADefaultDeviceAggregate`, `VPAUAggregateAudioDevice`) ; à défaut, lire
    la clé `private` de `kAudioAggregateDevicePropertyComposition`.
19. **AirPods** : canaux d'entrée visibles dès l'apparition dans la liste (sinon le micro est
    repris au prochain changement de la liste ou de configuration, pas immédiatement) ; délai de
    bascule A2DP → HFP lors du passage au micro des AirPods (trou plus long qu'avec un micro filaire).
20. **Retrait d'un écouteur HAL depuis sa propre file** (`AudioObjectRemovePropertyListenerBlock`
    appelé sur la file de rappel, dans `MicCapture.stop()` et `AudioDeviceWatcher.stop()`) : pas
    d'interblocage.

## À tester en priorité sur un Mac

1. `swift build -c release` puis `swift test` (corriger les éventuelles erreurs de compilation).
2. Spike S1 complet, surtout l'étape 6 (lancement depuis une app Tauri vide), puis S1-bis (Slack,
   Zoom, Meet).
3. Spike S2 sur les enregistrements de S1 (`--spike-transcribe`).
4. Spike S4 sur le Mac professionnel.
5. Essai du protocole à la main (ci-dessus), puis débranchement du casque pendant la capture.
6. Spike S1-ter (périphériques) : `--list-devices`, `set_input` pendant l'écoute, AirPods rangés
   puis ressortis, `play` sur les AirPods et sur la sortie par défaut.
