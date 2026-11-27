// Transcription : protocoles indépendants du moteur + chargement paresseux des modèles.
//
// L'implémentation concrète (Parakeet TDT v3 via FluidAudio, sur le Neural Engine) est dans
// FluidAudioBackend.swift, seul fichier qui importe FluidAudio. Le vocabulaire est appliqué
// après coup par `VocabularyCorrector` (VocabularyCorrector.swift).

import Foundation

/// Reconnaissance vocale sur un énoncé complet (16 kHz, mono, Float32).
protocol SpeechRecognizer: AnyObject {
    var name: String { get }
    func transcribe(_ samples: [Float]) async throws -> String
}

/// Diarisation hors ligne d'un fichier audio.
protocol SpeakerDiarizer: AnyObject {
    func diarize(fileURL: URL) async throws -> [RawSpeakerTurn]
}

/// Fournisseur de modèles (permet de remplacer FluidAudio sans toucher au reste).
protocol SpeechBackend: AnyObject {
    /// Nom annoncé dans `ready.asr`.
    var asrName: String { get }
    func loadRecognizer() async throws -> SpeechRecognizer
    /// Renvoie une fabrique : une instance de VAD (à état) par canal.
    func loadVADFactory() async throws -> () -> VoiceActivityDetector
    func loadDiarizer() async throws -> SpeakerDiarizer
}

enum SpeechModelError: Error, CustomStringConvertible {
    case unavailable(String)

    var description: String {
        switch self {
        case let .unavailable(why): return why
        }
    }
}

/// Charge les modèles une seule fois, à la demande, et les partage entre les canaux.
actor SpeechModels {
    let asrName: String

    private let backend: SpeechBackend
    private let forceEnergyVAD: Bool
    private var recognizerTask: Task<SpeechRecognizer, Error>?
    private var recognizerFailed = false
    private var failureReported = false
    private var vadTask: Task<(() -> VoiceActivityDetector)?, Never>?
    private var diarizerInstance: SpeakerDiarizer?
    private var onRecognizerFailure: ((Error) -> Void)?

    init(backend: SpeechBackend,
         forceEnergyVAD: Bool = ProcessInfo.processInfo.environment["SCRIBE_VAD"] == "energy") {
        self.backend = backend
        self.asrName = backend.asrName
        self.forceEnergyVAD = forceEnergyVAD
    }

    /// Rappel appelé une seule fois si le modèle ASR ne peut pas être chargé.
    func setRecognizerFailureHandler(_ handler: @escaping (Error) -> Void) {
        onRecognizerFailure = handler
    }

    /// Lance le chargement en arrière-plan (au démarrage du sidecar).
    func preload() {
        _ = startRecognizerLoad()
        _ = startVADLoad()
    }

    /// Libère les modèles d'ASR et de VAD (fin d'écoute) : la mémoire revient au niveau de repos.
    func unload() {
        recognizerTask = nil
        vadTask = nil
        recognizerFailed = false
        failureReported = false
    }

    /// Libère les modèles de diarisation (après usage).
    func releaseDiarizer() {
        diarizerInstance = nil
    }

    /// Après un échec (ex. pas de réseau au premier téléchargement), permet de réessayer.
    func retryIfFailed() {
        guard recognizerFailed else { return }
        recognizerTask = nil
        recognizerFailed = false
        failureReported = false
        _ = startRecognizerLoad()
    }

    /// Le reconnaisseur, ou `nil` si le modèle est indisponible.
    func recognizer() async -> SpeechRecognizer? {
        let task = startRecognizerLoad()
        do {
            return try await task.value
        } catch {
            recognizerFailed = true
            if !failureReported {
                failureReported = true
                Log.error("modèle ASR indisponible : \(error)")
                onRecognizerFailure?(error)
            }
            return nil
        }
    }

    /// Nouvelle instance de VAD pour un canal (Silero si possible, sinon énergie).
    func makeVAD() async -> VoiceActivityDetector {
        if let factory = await startVADLoad().value {
            return factory()
        }
        return EnergyVAD()
    }

    func diarizer() async throws -> SpeakerDiarizer {
        if let diarizerInstance { return diarizerInstance }
        let d = try await backend.loadDiarizer()
        diarizerInstance = d
        return d
    }

    // MARK: Interne

    private func startRecognizerLoad() -> Task<SpeechRecognizer, Error> {
        if let recognizerTask { return recognizerTask }
        let backend = self.backend
        let task = Task<SpeechRecognizer, Error> {
            let started = Date()
            let r = try await backend.loadRecognizer()
            Log.info("modèle ASR \(r.name) prêt en \(String(format: "%.1f", Date().timeIntervalSince(started))) s")
            return r
        }
        recognizerTask = task
        return task
    }

    private func startVADLoad() -> Task<(() -> VoiceActivityDetector)?, Never> {
        if let vadTask { return vadTask }
        let backend = self.backend
        let forceEnergy = forceEnergyVAD
        let task = Task<(() -> VoiceActivityDetector)?, Never> {
            if forceEnergy {
                Log.info("VAD énergie forcée (SCRIBE_VAD=energy)")
                return nil
            }
            do {
                let factory = try await backend.loadVADFactory()
                Log.info("VAD Silero prête")
                return factory
            } catch {
                Log.warn("VAD Silero indisponible (\(error)) : repli sur la VAD énergie")
                return nil
            }
        }
        vadTask = task
        return task
    }
}
