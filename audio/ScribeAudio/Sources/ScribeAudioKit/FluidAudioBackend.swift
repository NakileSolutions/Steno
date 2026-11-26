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

import FluidAudio
import Foundation

final class FluidAudioBackend: SpeechBackend {
    let asrName = "parakeet-tdt-v3"

    /// Dossier des modèles (`SCRIBE_MODELS_DIR`), sinon le cache par défaut de FluidAudio.
    private let modelsDirectory: URL?

    init(modelsDirectory: URL? = FluidAudioBackend.modelsDirectoryFromEnvironment()) {
        self.modelsDirectory = modelsDirectory
    }

    static func modelsDirectoryFromEnvironment() -> URL? {
        guard let path = ProcessInfo.processInfo.environment["SCRIBE_MODELS_DIR"], !path.isEmpty else {
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
        var audio = samples
        if audio.count < Self.minimumSamples {
            audio += [Float](repeating: 0, count: Self.minimumSamples - audio.count)
        }
        var state = TdtDecoderState.make(decoderLayers: await manager.decoderLayerCount)
        let result = try await manager.transcribe(audio, decoderState: &state, language: .french)
        return result.text
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
