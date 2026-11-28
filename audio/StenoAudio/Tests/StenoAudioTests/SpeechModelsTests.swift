@testable import StenoAudioKit
import XCTest

private final class FakeDiarizer: SpeakerDiarizer {
    func diarize(fileURL: URL) async throws -> [RawSpeakerTurn] { [] }
}

/// Counts model loads; nothing is downloaded or run.
private final class CountingBackend: SpeechBackend {
    let asrName = "fake"
    var diarizerLoads = 0

    func loadRecognizer() async throws -> SpeechRecognizer {
        throw SpeechModelError.unavailable("not used in this test")
    }

    func loadVADFactory() async throws -> () -> VoiceActivityDetector {
        throw SpeechModelError.unavailable("not used in this test")
    }

    func loadDiarizer() async throws -> SpeakerDiarizer {
        diarizerLoads += 1
        return FakeDiarizer()
    }
}

private final class FakeModel {}

private final class TrackedRecognizer: SpeechRecognizer, FinalRecognizer {
    let name = "fake"
    func transcribe(_ samples: [Float]) async throws -> String { "" }
}

private final class TrackedVAD: VoiceActivityDetector {
    let name = "silero"
    let frameSize = 512
    let model: FakeModel
    init(model: FakeModel) { self.model = model }
    func speechProbability(_ frame: [Float]) async throws -> Float { 0 }
}

private final class TrackedDiarizer: SpeakerDiarizer {
    func diarize(fileURL: URL) async throws -> [RawSpeakerTurn] { [] }
}

/// Records, at each load, which of the models loaded before are still alive.
private final class TrackingBackend: SpeechBackend {
    let asrName = "fake"
    private var loaded: [(String, () -> Bool)] = []
    private(set) var aliveAtLoad: [String: [String]] = [:]

    private func track(_ name: String, _ object: AnyObject) {
        aliveAtLoad[name] = loaded.filter { $0.1() }.map(\.0)
        loaded.append((name, { [weak object] in object != nil }))
    }

    func loadRecognizer() async throws -> SpeechRecognizer {
        let r = TrackedRecognizer()
        track("asr", r)
        return r
    }

    func loadVADFactory() async throws -> () -> VoiceActivityDetector {
        let model = FakeModel()
        track("vad", model)
        return { TrackedVAD(model: model) }
    }

    func loadDiarizer() async throws -> SpeakerDiarizer {
        let d = TrackedDiarizer()
        track("diarizer", d)
        return d
    }

    func loadFinalRecognizer(language: String) async throws -> FinalRecognizer {
        let r = TrackedRecognizer()
        track("final", r)
        return r
    }
}

final class SpeechModelsTests: XCTestCase {
    func testPrefetchFreesEachModelBeforeLoadingTheNext() async {
        let backend = TrackingBackend()
        let models = SpeechModels(backend: backend, forceEnergyVAD: false)
        let code = await Spikes.prefetchModels(models: models)
        XCTAssertEqual(code, 0)
        // Every model was loaded, and none of the earlier ones was still in memory by then
        // (they used to pile up to ~2.4 GB during `make install`).
        XCTAssertEqual(backend.aliveAtLoad.keys.sorted(), ["asr", "diarizer", "final", "vad"])
        for (name, alive) in backend.aliveAtLoad {
            XCTAssertEqual(alive, [], "still loaded when \(name) was loaded: \(alive)")
        }
    }

    func testDiarizerIsCachedUntilReleased() async throws {
        let backend = CountingBackend()
        let models = SpeechModels(backend: backend, forceEnergyVAD: true)
        let first = try await models.diarizer(captureRunning: false)
        let second = try await models.diarizer(captureRunning: false)
        XCTAssertTrue(first === second)
        XCTAssertEqual(backend.diarizerLoads, 1)

        await models.releaseDiarizer()
        _ = try await models.diarizer(captureRunning: false)
        XCTAssertEqual(backend.diarizerLoads, 2)
    }

    func testUnloadAlsoFreesTheDiarizer() async throws {
        let backend = CountingBackend()
        let models = SpeechModels(backend: backend, forceEnergyVAD: true)
        _ = try await models.diarizer(captureRunning: false)

        // End of listening: the diarizer must not survive it (it used to stay for the whole
        // life of the process when a diarization had failed).
        await models.unload()
        _ = try await models.diarizer(captureRunning: false)
        XCTAssertEqual(backend.diarizerLoads, 2, "unload() must drop the cached diarizer")
    }

    /// Loads the live ASR and VAD models, as a capture does, without keeping them.
    private func loadLiveModels(_ models: SpeechModels) async {
        _ = await models.recognizer()
        _ = await models.makeVAD()
    }

    func testRetranscriptionFreesLiveModelsWhenNoCaptureRuns() async throws {
        let backend = TrackingBackend()
        let models = SpeechModels(backend: backend, forceEnergyVAD: false)
        await loadLiveModels(models)

        // Just after `stop`: the deferred unload has not run yet when the core asks for the
        // retranscription. Parakeet and Silero must be gone before Canary is loaded.
        _ = try await models.finalRecognizer(language: "en", captureRunning: false)
        XCTAssertEqual(backend.aliveAtLoad["final"], [])
    }

    func testDiarizationFreesLiveModelsWhenNoCaptureRuns() async throws {
        let backend = TrackingBackend()
        let models = SpeechModels(backend: backend, forceEnergyVAD: false)
        await loadLiveModels(models)

        _ = try await models.diarizer(captureRunning: false)
        XCTAssertEqual(backend.aliveAtLoad["diarizer"], [])
    }

    func testBatchJobsKeepLiveModelsWhileACaptureRuns() async throws {
        let backend = TrackingBackend()
        let models = SpeechModels(backend: backend, forceEnergyVAD: false)
        await loadLiveModels(models)

        // A new meeting started right after the previous one: its models stay loaded.
        _ = try await models.finalRecognizer(language: "en", captureRunning: true)
        _ = try await models.diarizer(captureRunning: true)
        XCTAssertEqual(backend.aliveAtLoad["final"]?.sorted(), ["asr", "vad"])
        XCTAssertEqual(backend.aliveAtLoad["diarizer"]?.sorted(), ["asr", "vad"])
    }
}
