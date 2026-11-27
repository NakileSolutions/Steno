// Seul fichier qui dépend de FluidAudio (Apache 2.0). Tout le reste passe par les
// protocoles `SpeechBackend`, `SpeechRecognizer`, `VoiceActivityDetector`, `SpeakerDiarizer`.
//
// API relevée sur FluidAudio v0.18.4 (sources), l'API évolue vite :
// À VÉRIFIER (spike S2) :
//   - `AsrModels.downloadAndLoad(to:configuration:version:…)` : `.v3` = Parakeet TDT 0.6B v3
//     multilingue (FR inclus) ; emplacement du cache par défaut et sens exact de `to:` ;
//   - `AsrManager.transcribe(_:decoderState:language:)` exige un `TdtDecoderState` (le README
//     montre encore `transcribe(samples)`, qui n'existe plus) ; on en crée un neuf par énoncé ;
//   - `language: .french` : filtre de script (latin) seulement, à confirmer sans effet de bord
//     sur les termes anglais ;
//   - `VadManager.processStreamingChunk(_:state:config:returnSeconds:timeResolution:)` et
//     `VadManager.chunkSize` (4096 échantillons = 256 ms) ;
//   - `OfflineDiarizerManager.prepareModels()` puis `process(_ url:)`, segments
//     `speakerId` / `startTimeSeconds` / `endTimeSeconds` ;
//   - placement sur le Neural Engine (configuration CoreML par défaut de FluidAudio).

import CoreML
import FluidAudio
import Foundation

final class FluidAudioBackend: SpeechBackend {
    let asrName = "parakeet-tdt-v3"

    /// Dossier des modèles (`STENO_MODELS_DIR`), sinon le cache par défaut de FluidAudio.
    private let modelsDirectory: URL?

    init(modelsDirectory: URL? = FluidAudioBackend.modelsDirectoryFromEnvironment()) {
        self.modelsDirectory = modelsDirectory
    }

    static func modelsDirectoryFromEnvironment() -> URL? {
        guard let path = ProcessInfo.processInfo.environment["STENO_MODELS_DIR"], !path.isEmpty else {
            return nil
        }
        return URL(fileURLWithPath: path, isDirectory: true)
    }

    func loadRecognizer() async throws -> SpeechRecognizer {
        let models = try await AsrModels.downloadAndLoad(
            to: modelsDirectory?.appendingPathComponent("parakeet-tdt-v3", isDirectory: true),
            version: .v3
        )
        let manager = AsrManager(config: .default)
        try await manager.loadModels(models)
        let recognizer = ParakeetRecognizer(manager: manager)
        // Préchauffage : la première inférence CoreML compile/charge le graphe sur l'ANE.
        _ = try? await recognizer.transcribe([Float](repeating: 0, count: AudioFormat.sampleRateInt))
        return recognizer
    }

    func loadVADFactory() async throws -> () -> VoiceActivityDetector {
        let manager = try await VadManager(config: VadConfig(defaultThreshold: 0.5))
        return { SileroVAD(manager: manager) }
    }

    func loadDiarizer() async throws -> SpeakerDiarizer {
        let manager = OfflineDiarizerManager()
        try await manager.prepareModels()
        return FluidDiarizer(manager: manager)
    }

    /// Charge Canary 1B v2 (int4, seule variante publiée) sur CPU + GPU.
    ///
    /// Le chargement par défaut de FluidAudio vise le Neural Engine : la compilation ANE de
    /// l'encodeur a dépassé 35 min ici sans aboutir. Sur le GPU, le modèle se charge en quelques
    /// secondes. `STENO_CANARY_UNITS=ane` rétablit le Neural Engine.
    static func loadCanary(_ lang: CanaryLanguage) async throws -> CanaryManager {
        let dir = try await CanaryModels.download(precision: .int4)
        let cpu = MLModelConfiguration()
        cpu.computeUnits = .cpuOnly
        let accel = MLModelConfiguration()
        accel.computeUnits = ProcessInfo.processInfo.environment["STENO_CANARY_UNITS"] == "ane"
            ? .cpuAndNeuralEngine : .cpuAndGPU
        func model(_ name: String, _ config: MLModelConfiguration) throws -> MLModel {
            try MLModel(contentsOf: dir.appendingPathComponent("\(name).mlmodelc"), configuration: config)
        }
        let models = CanaryModels(
            preprocessor: try model(ModelNames.Canary.preprocessor, cpu),
            encoder: try model(ModelNames.Canary.encoderInt4, accel),
            decoder: try model(ModelNames.Canary.decoderInt4, accel),
            projection: try model(ModelNames.Canary.projection, accel),
            tokenizer: try Tokenizer(vocabPath: dir.appendingPathComponent(ModelNames.Canary.vocabularyFile))
        )
        return try CanaryManager(models: models, source: lang, target: lang)
    }

    func loadFinalRecognizer(language: String) async throws -> FinalRecognizer {
        guard let lang = CanaryLanguage(rawValue: language) else {
            throw SpeechModelError.unavailable("langue non prise en charge par Canary : \(language)")
        }
        let manager = try await Self.loadCanary(lang)
        return CanaryRecognizer(manager: manager, language: language)
    }
}

// MARK: - Retranscription : Canary 1B v2 (langue imposée)

final class CanaryRecognizer: FinalRecognizer {
    let name: String
    private let manager: CanaryManager

    init(manager: CanaryManager, language: String) {
        self.manager = manager
        self.name = "canary-1b-v2-\(language)"
    }

    func transcribe(_ samples: [Float]) async throws -> String {
        try await manager.transcribe(audio: samples)
    }
}

// MARK: - ASR : Parakeet TDT v3

final class ParakeetRecognizer: SpeechRecognizer {
    let name = "parakeet-tdt-v3"
    private let manager: AsrManager

    /// Parakeet tolère mal les énoncés très courts : on complète par du silence.
    private static let minimumSamples = AudioFormat.sampleRateInt // 1 s

    init(manager: AsrManager) {
        self.manager = manager
    }

    func transcribe(_ samples: [Float]) async throws -> String {
        try await transcribeDetailed(samples).text
    }

    func transcribeDetailed(_ samples: [Float]) async throws -> (text: String, confidence: Float?) {
        var audio = samples
        if audio.count < Self.minimumSamples {
            audio += [Float](repeating: 0, count: Self.minimumSamples - audio.count)
        }
        var state = TdtDecoderState.make(decoderLayers: await manager.decoderLayerCount)
        let result = try await manager.transcribe(audio, decoderState: &state, language: .french)
        return (result.text, result.confidence)
    }
}

// MARK: - VAD : Silero

final class SileroVAD: VoiceActivityDetector {
    let name = "silero"
    let frameSize = VadManager.chunkSize

    private let manager: VadManager
    private var state: VadStreamState

    init(manager: VadManager) {
        self.manager = manager
        self.state = VadStreamState.initial()
    }

    func speechProbability(_ frame: [Float]) async throws -> Float {
        let result = try await manager.processStreamingChunk(frame, state: state)
        state = result.state
        return result.probability
    }
}

// MARK: - Diarisation hors ligne

final class FluidDiarizer: SpeakerDiarizer {
    private let manager: OfflineDiarizerManager

    init(manager: OfflineDiarizerManager) {
        self.manager = manager
    }

    func diarize(fileURL: URL) async throws -> [RawSpeakerTurn] {
        let result = try await manager.process(fileURL)
        return result.segments.map {
            RawSpeakerTurn(speakerId: $0.speakerId,
                           start: Double($0.startTimeSeconds),
                           end: Double($0.endTimeSeconds))
        }
    }
}

// MARK: - Spike : comparaison de modèles (seul fichier autorisé à dépendre de FluidAudio)

extension Spikes {
    /// Parakeet v3 (filtre « french ») contre Nemotron Unified 0.6B (langue imposée : fr).

    static func compareASR(files: [URL]) async -> Int32 {
        do {
            let t0 = Date()
            let parakeet = AsrManager(config: .default)
            try await parakeet.loadModels(try await AsrModels.downloadAndLoad(version: .v3))
            print(String(format: "Parakeet v3 chargé en %.1f s", Date().timeIntervalSince(t0)))
            let t1 = Date()
            let canary = try await FluidAudioBackend.loadCanary(.french)
            print(String(format: "Canary 1B v2 (fr) chargé en %.1f s", Date().timeIntervalSince(t1)))
            for file in files {
                let samples = try loadMono16k(file)
                print("\n== \(file.lastPathComponent) (\(String(format: "%.1f", Double(samples.count) / 16000)) s) ==")
                var state = TdtDecoderState.make(decoderLayers: await parakeet.decoderLayerCount)
                var s = Date()
                let p = try await parakeet.transcribe(samples, decoderState: &state, language: .french)
                print(String(format: "Parakeet v3 (%.2f s, confiance %.2f) : %@", Date().timeIntervalSince(s), p.confidence, p.text))
                s = Date()
                let n = try await canary.transcribe(audio: samples)
                print(String(format: "Canary fr (%.2f s) : %@", Date().timeIntervalSince(s), n))
            }
            return 0
        } catch {
            print("ERREUR : \(error)")
            return 1
        }
    }

}
