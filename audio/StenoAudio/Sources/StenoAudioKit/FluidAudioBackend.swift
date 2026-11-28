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

/// Where each model lives under `STENO_MODELS_DIR`: one subfolder per model, so a single folder
/// holds everything `--prefetch-models` prepares (nothing goes to FluidAudio's default cache).
struct ModelLocations: Equatable {
    /// Parakeet TDT v3 (live transcription), passed to `AsrModels.downloadAndLoad(to:)`.
    let parakeet: URL
    /// Silero VAD, passed to `VadManager(modelDirectory:)` (FluidAudio adds `Models/<repo>`).
    let vad: URL
    /// Offline diarizer, passed to `OfflineDiarizerManager.prepareModels(directory:)`.
    let diarizer: URL
    /// Canary 1B v2 (end-of-meeting retranscription): the download root; the models end up in
    /// `canaryModels`, the repo folder FluidAudio creates under it.
    let canary: URL

    init(base: URL) {
        parakeet = base.appendingPathComponent("parakeet-tdt-v3", isDirectory: true)
        vad = base.appendingPathComponent("silero-vad", isDirectory: true)
        diarizer = base.appendingPathComponent("diarizer", isDirectory: true)
        canary = base.appendingPathComponent("canary-1b-v2", isDirectory: true)
    }

    var canaryModels: URL {
        canary.appendingPathComponent(Repo.canary1bV2.folderName, isDirectory: true)
    }
}

final class FluidAudioBackend: SpeechBackend {
    let asrName = "parakeet-tdt-v3"

    /// Models folder (`STENO_MODELS_DIR`), otherwise FluidAudio's default cache.
    private let modelsDirectory: URL?
    private var locations: ModelLocations? { modelsDirectory.map(ModelLocations.init(base:)) }

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
            to: locations?.parakeet,
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
        let config = VadConfig(defaultThreshold: 0.5)
        let manager: VadManager
        if let dir = locations?.vad {
            manager = try await VadManager(config: config, modelDirectory: dir)
        } else {
            manager = try await VadManager(config: config)
        }
        return { SileroVAD(manager: manager) }
    }

    func loadDiarizer() async throws -> SpeakerDiarizer {
        let manager = OfflineDiarizerManager()
        try await manager.prepareModels(directory: locations?.diarizer)
        return FluidDiarizer(manager: manager)
    }

    /// Downloads Canary 1B v2 (int4) if needed and returns the folder holding its models.
    /// `CanaryModels.download` only knows FluidAudio's default cache, so with `STENO_MODELS_DIR`
    /// the same download goes through `ModelHub` into our own folder.
    static func canaryDirectory(modelsDirectory: URL?) async throws -> URL {
        guard let locations = modelsDirectory.map(ModelLocations.init(base:)) else {
            return try await CanaryModels.download(precision: .int4)
        }
        let dir = locations.canaryModels
        if !CanaryModels.modelsExist(at: dir, precision: .int4) {
            try await ModelHub.download(.canary1bV2, to: locations.canary, variant: CanaryPrecision.int4.rawValue)
        }
        return dir
    }

    /// Charge Canary 1B v2 (int4, seule variante publiée) sur CPU + GPU.
    ///
    /// Le chargement par défaut de FluidAudio vise le Neural Engine : la compilation ANE de
    /// l'encodeur a dépassé 35 min ici sans aboutir. Sur le GPU, le modèle se charge en quelques
    /// secondes. `STENO_CANARY_UNITS=ane` rétablit le Neural Engine.
    static func loadCanary(
        _ lang: CanaryLanguage,
        modelsDirectory: URL? = FluidAudioBackend.modelsDirectoryFromEnvironment()
    ) async throws -> CanaryManager {
        let dir = try await canaryDirectory(modelsDirectory: modelsDirectory)
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
        let manager = try await Self.loadCanary(lang, modelsDirectory: modelsDirectory)
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
            let parakeetDirectory = FluidAudioBackend.modelsDirectoryFromEnvironment()
                .map { ModelLocations(base: $0).parakeet }
            try await parakeet.loadModels(try await AsrModels.downloadAndLoad(to: parakeetDirectory, version: .v3))
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
