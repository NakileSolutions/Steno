# scribe-audio — sidecar audio de Scribe

Binaire Swift lancé par le cœur de Scribe. Il ne fait que de l'audio vers du texte :

- capture du **micro** (canal `me`) avec `AVAudioEngine` et voice processing (annulation d'écho) ;
- capture du **son de Teams** (canal `them`) par un *process tap* Core Audio sur tout l'arbre de
  processus Teams (macOS 14.2+) ;
- rééchantillonnage en 16 kHz mono, **VAD** (Silero via FluidAudio, repli par énergie),
  découpage en énoncés (fin après `silence_ms` de silence ou 25 s), **transcription Parakeet TDT v3**
  sur le Neural Engine, correction par le vocabulaire ;
- détection des appels Teams et lecture du titre de la réunion (accessibilité) ;
- diarisation hors ligne de `them.wav` à la demande.

Il dialogue avec le cœur en **JSON lines** sur stdin/stdout, selon `docs/contracts.md` §2.
Les journaux vont **uniquement sur stderr**. Aucune donnée ne quitte le Mac (seul le premier
lancement télécharge les modèles depuis Hugging Face, via FluidAudio).

## Prérequis

- macOS 14.2 ou plus récent, Apple Silicon ;
- Xcode 16 (Swift 5.10 / 6.0) ou plus récent ;
- accès réseau au premier lancement (dépendance FluidAudio et modèles).

## Compilation

```sh
cd audio/ScribeAudio
swift build -c release            # binaire : .build/release/scribe-audio
swift test                        # tests sans matériel (protocole, vocabulaire, segmenteur…)
.build/release/scribe-audio --prefetch-models   # télécharge et compile les modèles (≈ 1 Go)
```

`Package.swift` embarque `Info.plist` dans le binaire (section `__TEXT,__info_plist`) pour que les
descriptions d'usage existent même quand le sidecar est lancé seul.

Variables d'environnement utiles :

| Variable | Effet |
|---|---|
| `SCRIBE_TAP_MODE=global` | plan B : tap sur tout le son du Mac sauf Scribe (si Teams sort l'audio par un processus non repéré) |
| `SCRIBE_VAD=energy` | VAD par énergie au lieu de Silero |
| `SCRIBE_MODELS_DIR=<dossier>` | dossier des modèles (sinon le cache par défaut de FluidAudio) |
| `SCRIBE_AUDIO_DEBUG=1` | journaux détaillés sur stderr |

Autres commandes : `--version`, `--help`, `--list-audio-processes` (diagnostic des processus
audio vus par Core Audio), `--prefetch-models`, et les deux modes spike décrits plus bas.

## Intégration dans Scribe.app (`externalBin` Tauri)

Tauri attend le binaire suffixé par le triplet de la cible :

```sh
cd audio/ScribeAudio
swift build -c release --arch arm64
mkdir -p ../../app/src-tauri/binaries
cp .build/arm64-apple-macosx/release/scribe-audio \
   ../../app/src-tauri/binaries/scribe-audio-aarch64-apple-darwin
```

Dans `app/src-tauri/tauri.conf.json` :

```json
{ "bundle": { "externalBin": ["binaries/scribe-audio"] } }
```

Tauri copie alors le binaire dans `Scribe.app/Contents/MacOS/scribe-audio` et le signe avec l'app
(signature ad hoc suffisante). Côté Rust, le cœur le lance avec `tauri-plugin-shell`
(`app.shell().sidecar("scribe-audio")`) ou `std::process::Command`, écrit les commandes sur stdin
et lit stdout ligne par ligne. Fermer stdin (ou envoyer `{"cmd":"shutdown"}`) arrête le sidecar
proprement.

L'`Info.plist` de **Scribe.app** (`app/src-tauri/Info.plist`) doit contenir les mêmes clés que
`audio/ScribeAudio/Info.plist` : `NSMicrophoneUsageDescription` et `NSAudioCaptureUsageDescription`.

### Permissions TCC : qui les porte ?

macOS attribue les demandes de permission (micro, enregistrement audio du système) au **processus
responsable**, c'est-à-dire l'app qui a lancé le sidecar. Les permissions sont donc demandées et
accordées **à Scribe.app**, avec les textes de son `Info.plist` ; le sidecar en hérite tant qu'il est
lancé par l'app. C'est le processus parent qui en est responsable : le sidecar ne fait que constater
l'état (`permissions`) et signaler les refus (`mic_denied`, `system_audio_denied`).

Conséquences pratiques :

- lancé depuis un terminal (spikes, `cargo tauri dev`), c'est **le terminal** (Terminal, iTerm…) qui
  reçoit les demandes et doit être autorisé ;
- l'accessibilité (titre de la fenêtre Teams) est accordée à Scribe.app dans Réglages Système ›
  Confidentialité et sécurité › Accessibilité ;
- **à vérifier au spike S1** : que l'autorisation accordée à Scribe.app couvre bien le sidecar.
  **Plan B** si ce n'est pas le cas : faire la capture en Rust (`objc2-core-audio` pour le tap,
  `objc2-avf-audio` pour le micro) dans le processus principal de l'app, et ne garder dans le sidecar
  que la VAD et la transcription (l'audio lui serait alors envoyé par un tube).

## Spikes à exécuter sur le Mac (phase 0)

### S1 — Capture (bloquant)

But : deux pistes audibles pendant un vrai appel Teams, permissions valables pour le sidecar.

1. Compiler : `swift build -c release`.
2. Lancer un appel Teams (réunion avec un collègue, ou Paramètres › Appareils › « Passer un appel
   test »). Parler et faire parler l'autre participant.
3. Repérer les processus audio de Teams :
   `.build/release/scribe-audio --list-audio-processes`
   → noter les bundles marqués « Teams : oui » et vérifier qu'aucun autre processus (ex.
   `com.apple.WebKit.GPU`, assistant Edge WebView) ne joue le son de l'appel sans être repéré.
4. Depuis le terminal (le terminal doit être autorisé pour le micro et l'audio système) :
   `.build/release/scribe-audio --spike-record 60 ~/Desktop/s1`
   → le niveau de « moi » et « eux » s'affiche chaque seconde ; à la fin, ouvrir
   `~/Desktop/s1/me.wav` et `them.wav` dans QuickTime : les deux pistes doivent être audibles,
   séparées, et l'écho de « eux » quasi absent de `me.wav` (AEC).
5. Pendant l'enregistrement, débrancher puis rebrancher le casque : stderr doit indiquer
   « micro reparti » en moins de 5 s.
6. **Critère décisif** : refaire l'étape 4 depuis une **app Tauri vide** qui lance le sidecar avec
   les mêmes arguments (`sidecar("scribe-audio").args(["--spike-record","60","/tmp/s1"])`), après
   avoir **retiré** l'autorisation du terminal. Vérifier que les invites système nomment l'app, que
   les deux WAV sont audibles, et que Réglages Système liste l'app (pas `scribe-audio`).
7. Si `them.wav` est silencieux alors que Teams joue du son : réessayer avec
   `SCRIBE_TAP_MODE=global` (tap global) ; si ça marche, l'audio passe par un processus non repéré
   (étape 3) → noter son bundle dans `docs/spikes.md`.

### S2 — Transcription

But : français lisible, noms techniques compris, moins d'une seconde par phrase.

1. `.build/release/scribe-audio --prefetch-models` (premier téléchargement des modèles).
2. `.build/release/scribe-audio --spike-transcribe ~/Desktop/s1/them.wav`
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
3. Lancer `printf '{"cmd":"permissions"}\n' | .build/release/scribe-audio` : on doit voir `ready`
   puis deux messages `permissions` (micro `granted`, accessibilité `granted` une fois autorisée).
4. Refaire S1 étape 4. Si la création du tap échoue, la sortie indique « accès à l'audio système
   refusé » (protocole : `system_audio_denied`). **Si une permission est bloquée : arrêter et
   prévenir l'utilisateur** (PLAN §13).

### Essai du protocole à la main

```sh
( echo '{"cmd":"start","vocabulary":["Scribe","ms-booking"],"silence_ms":1000,"keep_audio":true,"audio_dir":"/tmp/scribe-essai"}'
  sleep 30
  echo '{"cmd":"aside","on":true}'; sleep 5; echo '{"cmd":"aside","on":false}'
  sleep 5
  echo '{"cmd":"stop"}'
  sleep 3
  echo '{"cmd":"diarize","meeting_id":"essai","audio_dir":"/tmp/scribe-essai"}'
  sleep 60
) | .build/release/scribe-audio
```

Attendu : `ready`, `permissions`, `state listening`, des `level` (4 par seconde au plus), des
`segment` (ceux dits pendant l'aparté avec `"private":true`), `state stopped`, puis `diarization`.
La fin de stdin arrête le sidecar.

## Organisation du code

| Fichier | Rôle |
|---|---|
| `Sources/ScribeAudio/main.swift` | point d'entrée (appelle `ScribeAudioMain.run`) |
| `ScribeAudioMain.swift` | arguments, thread de lecture de stdin, signaux, boucle de commandes |
| `Protocol.swift` | messages des deux sens (Codable), erreurs `bad_command`, horodatage ISO 8601 |
| `Output.swift` | sortie stdout sérialisée (une ligne par message), journaux stderr, utilitaires |
| `Controller.swift` | traitement des commandes, cycle de vie de la capture |
| `CaptureSession.swift` | micro + tap + deux canaux, recréation du tap, aparté, niveaux |
| `ChannelPipeline.swift` | par canal : trous du flux, WAV, VAD, segmenteur, ASR, vocabulaire |
| `TeamsMonitor.swift` | détection d'appel Teams (toutes les 2 s), titre de réunion (accessibilité) |
| `ProcessTap.swift` | process tap Core Audio + agrégat privé + IOProc |
| `MicCapture.swift` | micro `AVAudioEngine` avec AEC, redémarrage sur changement de périphérique |
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
| `Spikes.swift` | modes `--spike-record`, `--spike-transcribe`, diagnostic, préchargement |

Choix notables :

- `ts` d'un segment = heure locale (avec fuseau) de la **fin** de l'énoncé ; `t0`/`t1` = secondes
  depuis `start`, sur une horloge monotone. Quand le tap disparaît (Teams redémarre, veille), le
  trou est détecté et comblé (silence dans le WAV, horloge avancée) : `t0`/`t1` et `them.wav`
  restent alignés sur le `start`, ce qui permet au cœur de croiser diarisation et segments.
- Un énoncé `me` est `private` si l'aparté a été actif à un moment quelconque pendant l'énoncé.
- Fin d'appel Teams déclarée après deux relevés négatifs consécutifs (≈ 4 s) ; le délai de grâce
  de 60 s reste du ressort du cœur.
- Le titre de réunion est figé dès qu'un titre plausible est trouvé pendant l'appel.
- Codes d'erreur de `diarize` : `bad_command` (them.wav absent), `model_missing` (modèles de
  diarisation indisponibles), `asr_failed` (échec du traitement).
- `stop` attend la transcription des énoncés en cours (au plus 15 s) avant d'émettre
  `state stopped`.

## Points À VÉRIFIER sur le Mac

Le code a été écrit sans toolchain Swift ni SDK macOS : il doit être compilé et éprouvé sur le Mac.
Points incertains, par ordre de priorité :

1. **Héritage TCC** par le sidecar lancé depuis Scribe.app (S1, étape 6) ; sinon plan B Rust.
2. **API FluidAudio** (relevée sur la v0.18.4, détails en tête de `FluidAudioBackend.swift`) :
   `AsrModels.downloadAndLoad(to:version:)`, `AsrManager.transcribe(_:decoderState:language:)`
   (le README de FluidAudio montre encore `transcribe(samples)`), `language: .french`,
   `VadManager.processStreamingChunk`, `OfflineDiarizerManager.prepareModels()/process(_:)`.
3. **Processus audio de Teams** : bundles réels (`com.microsoft.teams2*` ?) et existence d'un
   processus de rendu hors préfixe et hors descendance (ex. XPC WebKit) → `--list-audio-processes`.
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
8. **Titres de fenêtre** du nouveau Teams en français (heuristique `TeamsTitleHeuristics`) et
   attribut `AXMain`.
9. `proc_pidinfo` / `PROC_PIDTBSDINFO` accessibles via `import Darwin` (descendance des processus).
10. Réception de `NSWorkspace.didWakeNotification` dans un binaire sans `NSApplication`
    (boucle `RunLoop.main.run()`).
11. Option d'édition de liens `-sectcreate __TEXT __info_plist` (`unsafeFlags`) acceptée par
    `swift build` pour le paquet racine.
12. Latence réelle fin de phrase → segment (< 3 s visé) avec `silence_ms = 1000` et des trames VAD
    de 256 ms.

## À tester en priorité sur un Mac

1. `swift build -c release` puis `swift test` (corriger les éventuelles erreurs de compilation).
2. Spike S1 complet, surtout l'étape 6 (lancement depuis une app Tauri vide).
3. Spike S2 sur les enregistrements de S1 (`--spike-transcribe`).
4. Spike S4 sur le Mac professionnel.
5. Essai du protocole à la main (ci-dessus), puis débranchement du casque pendant la capture.
