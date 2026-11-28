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
    /// Texte et confiance du modèle (0…1, nil si inconnue).
    func transcribeDetailed(_ samples: [Float]) async throws -> (text: String, confidence: Float?)
}

extension SpeechRecognizer {
    func transcribeDetailed(_ samples: [Float]) async throws -> (text: String, confidence: Float?) {
        (try await transcribe(samples), nil)
    }
}

/// Retranscription de fin de réunion : modèle plus lent et plus précis, langue imposée.
protocol FinalRecognizer: AnyObject {
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
    /// `language` : code ISO (`fr`…). Téléchargé au premier usage.
    func loadFinalRecognizer(language: String) async throws -> FinalRecognizer
}

extension SpeechBackend {
    func loadFinalRecognizer(language: String) async throws -> FinalRecognizer {
        throw SpeechModelError.unavailable("retranscription non prise en charge par \(asrName)")
    }
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
         forceEnergyVAD: Bool = ProcessInfo.processInfo.environment["STENO_VAD"] == "energy") {
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

    /// Frees the ASR, VAD and diarization models (end of listening): memory goes back to its
    /// idle level. A diarization already running keeps its own reference until it ends.
    func unload() {
        releaseLiveModels()
        diarizerInstance = nil
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

    /// Retranscription model (loaded on demand, never kept: it weighs ~1 GB).
    ///
    /// `captureRunning: false` frees the live ASR and VAD models first. Right after `stop`, the
    /// core starts the retranscription at once, while the deferred `unload()` of the end of
    /// listening may not have run yet: both models would otherwise sit in memory together.
    func finalRecognizer(language: String, captureRunning: Bool) async throws -> FinalRecognizer {
        if !captureRunning { releaseLiveModels() }
        let started = Date()
        let r = try await backend.loadFinalRecognizer(language: language)
        Log.info("modèle de retranscription \(r.name) prêt en \(String(format: "%.1f", Date().timeIntervalSince(started))) s")
        return r
    }

    /// Diarization models, kept until `releaseDiarizer()` or `unload()`. `captureRunning: false`
    /// frees the live ASR and VAD models first, as for `finalRecognizer`.
    func diarizer(captureRunning: Bool) async throws -> SpeakerDiarizer {
        if !captureRunning { releaseLiveModels() }
        if let diarizerInstance { return diarizerInstance }
        let d = try await backend.loadDiarizer()
        diarizerInstance = d
        return d
    }

    // MARK: Interne

    /// Frees the live ASR and VAD models (not the diarizer). A capture that already holds
    /// them keeps its own references; the next one loads them again.
    private func releaseLiveModels() {
        recognizerTask = nil
        vadTask = nil
        recognizerFailed = false
        failureReported = false
    }

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
                Log.info("VAD énergie forcée (STENO_VAD=energy)")
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
