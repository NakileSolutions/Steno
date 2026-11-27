// Contrôleur du mode protocole : reçoit les commandes, pilote la session de capture,
// relaie la surveillance des apps de visio (`watch` → `call`) et les permissions.

import CoreAudio
import Foundation

@available(macOS 14.2, *)
actor Controller {
    private let output: Output
    private let models: SpeechModels
    private let permissions = PermissionsTracker()
    private let monitor = CallMonitor()
    private var session: CaptureSession?
    /// `start.apps` de la session en cours (vide = toutes les apps surveillées).
    private var captureRequest: [String] = []
    private var shuttingDown = false

    init(output: Output, backend: SpeechBackend = FluidAudioBackend()) {
        self.output = output
        self.models = SpeechModels(backend: backend)
    }

    /// Démarrage : `ready`, `permissions`, surveillance des apps, préchargement des modèles.
    func boot() async {
        output.send(.ready(version: SidecarInfo.version, asr: models.asrName))
        output.send(.permissions(permissions.snapshot()))

        let output = self.output
        await models.setRecognizerFailureHandler { error in
            output.send(.error(code: .modelMissing,
                               message: "Modèle de transcription indisponible : \(error). Lancez `make install` (téléchargement des modèles) puis réessayez.",
                               recoverable: false))
        }

        monitor.onCallChange = { app, active, title in
            output.send(.call(active: active, app: app.id, title: title))
        }
        monitor.onSnapshot = { [weak self] snapshot in
            Task { await self?.snapshotChanged(snapshot) }
        }
        monitor.start()

        // Modèles chargés au premier `start` (RAM au repos minimale, objectif < 120 Mo) ;
        // SCRIBE_PRELOAD_MODELS=1 les charge dès le lancement (première phrase plus rapide).
        if ProcessInfo.processInfo.environment["SCRIBE_PRELOAD_MODELS"] == "1" {
            await models.preload()
        }
        Log.info("prêt (scribe-audio \(SidecarInfo.version), ASR \(models.asrName))")
    }

    // MARK: Commandes

    func handle(line: String) async {
        switch CommandParser.parse(line) {
        case let .success(command):
            await handle(command)
        case let .failure(error):
            Log.warn("commande rejetée : \(error.message)")
            output.send(.error(code: .badCommand, message: error.message, recoverable: true))
        }
    }

    func handle(_ command: Command) async {
        guard !shuttingDown else { return }
        switch command {
        case let .watch(apps):
            // Le relevé fait aussitôt par le moniteur met à jour le tap si une capture tourne.
            monitor.setApps(apps)
            Log.info("apps surveillées : \(apps.isEmpty ? "aucune" : apps.map(\.id).joined(separator: ", "))")

        case let .start(options):
            await start(options)

        case .stop:
            await stop()

        case let .aside(on):
            session?.setAside(on)
            Log.info("aparté \(on ? "activé" : "désactivé")")

        case let .wakeTest(seconds):
            // En tâche séparée : la boucle de commandes reste disponible pendant l'enregistrement.
            Task { await self.wakeTest(seconds: seconds) }

        case let .diarize(meetingId, audioDir):
            Task { await self.diarize(meetingId: meetingId, audioDir: audioDir) }

        case .permissions:
            output.send(.permissions(permissions.snapshot()))

        case .shutdown:
            await shutdown(reason: "commande shutdown")
        }
    }

    private func start(_ options: StartOptions) async {
        if session != nil {
            Log.warn("start reçu alors que la capture tourne déjà : ignoré")
            output.send(.state(.listening))
            return
        }
        await models.retryIfFailed()
        await models.preload()
        let config = CaptureSession.Config(
            vocabulary: options.vocabulary,
            silenceMs: options.silenceMs,
            keepAudio: options.keepAudio,
            audioDir: options.audioDir.map { URL(fileURLWithPath: $0, isDirectory: true) }
        )
        let s = CaptureSession(config: config, output: output, permissions: permissions, models: models)
        session = s
        captureRequest = options.apps
        // Relevé frais : le dernier relevé du minuteur peut dater de 2 s ou précéder un `watch`.
        let snapshot = monitor.scanNow()
        let selection = CaptureAppSelection.resolve(requested: captureRequest, watched: snapshot.apps.map(\.app))
        if !selection.unknown.isEmpty {
            Log.warn("start.apps : app(s) non surveillée(s) ignorée(s) : \(selection.unknown.joined(separator: ", "))")
        }
        await s.start(tapSource: tapSource(for: snapshot))
        Log.info("capture démarrée (apps \(selection.apps.map(\.id).joined(separator: ", ")), silence \(options.silenceMs) ms, vocabulaire \(options.vocabulary.count) mots, audio \(options.keepAudio ? "conservé" : "non conservé"))")
        output.send(.state(.listening))
    }

    private func stop() async {
        guard let s = session else {
            output.send(.state(.stopped))
            return
        }
        session = nil
        // Vide les énoncés en cours ; au-delà de 15 s (modèle encore en chargement…), on
        // annonce l'arrêt quand même et les derniers segments arriveront ensuite.
        let complete = await runWithTimeout(seconds: 15) { await s.stop() }
        if !complete { Log.warn("vidage des segments plus long que prévu : arrêt annoncé avant la fin") }
        Log.info("capture arrêtée")
        output.send(.state(.stopped))
        // Retour au niveau de repos : les modèles seront rechargés au prochain `start`.
        await models.unload()
    }

    private func wakeTest(seconds: Double) async {
        let samples: [Float]
        if let s = session {
            samples = await s.recordMic(seconds: seconds)
        } else {
            let mic = await PermissionsTracker.requestMicrophoneIfNeeded()
            guard mic != .denied else {
                output.send(.error(code: .micDenied, message: "Accès au micro refusé : test impossible.", recoverable: false))
                return
            }
            do {
                samples = try await MicRecorder.record(seconds: seconds)
            } catch {
                output.send(.error(code: .deviceChanged, message: "Micro indisponible pour le test : \(error)", recoverable: true))
                return
            }
        }
        guard let recognizer = await models.recognizer() else { return } // model_missing déjà émis
        do {
            // Transcription brute, sans correction par le vocabulaire (c'est ce qu'on teste).
            let text = try await recognizer.transcribe(samples)
            output.send(.wakeTest(text: text.trimmingCharacters(in: .whitespacesAndNewlines)))
        } catch {
            output.send(.error(code: .asrFailed, message: "Transcription du test impossible : \(error)", recoverable: true))
        }
    }

    private func diarize(meetingId: String, audioDir: String) async {
        let url: URL
        do {
            url = try diarizationInput(audioDir: audioDir)
        } catch {
            output.send(.error(code: .badCommand, message: "Diarisation impossible : \(error)", recoverable: true))
            return
        }
        let diarizer: SpeakerDiarizer
        do {
            diarizer = try await models.diarizer()
        } catch {
            Log.error("modèles de diarisation : \(error)")
            output.send(.error(code: .modelMissing, message: "Modèles de diarisation indisponibles : \(error)", recoverable: true))
            return
        }
        do {
            let turns = try await runDiarization(fileURL: url, diarizer: diarizer)
            output.send(.diarization(meetingId: meetingId, turns: turns))
            await models.releaseDiarizer()
        } catch {
            Log.error("diarisation : \(error)")
            output.send(.error(code: .asrFailed, message: "Diarisation impossible : \(error)", recoverable: true))
        }
    }

    // MARK: Apps surveillées

    private func snapshotChanged(_ snapshot: CallSnapshot) {
        // La session ignore une source identique à la précédente.
        session?.updateTapSource(tapSource(for: snapshot))
    }

    /// Processus à taper : ceux des apps de `start.apps` (ou de toutes les apps surveillées).
    private func tapSource(for snapshot: CallSnapshot) -> TapSource {
        let apps = CaptureAppSelection.resolve(requested: captureRequest, watched: snapshot.apps.map(\.app)).apps
        return TapSource(appNames: apps.map(\.name), processes: snapshot.processObjects(forApps: apps.map(\.id)))
    }

    // MARK: Arrêt

    /// Arrêt propre : capture coupée (segments vidés si possible en 5 s), sortie vidée, exit(0).
    func shutdown(reason: String) async {
        guard !shuttingDown else { return }
        shuttingDown = true
        Log.info("arrêt (\(reason))")
        monitor.stop()
        if let s = session {
            session = nil
            await runWithTimeout(seconds: 5) { await s.stop() }
            output.send(.state(.stopped))
        }
        output.drain()
        exit(0)
    }
}
