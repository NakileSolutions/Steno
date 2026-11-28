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

final class SpeechModelsTests: XCTestCase {
    func testDiarizerIsCachedUntilReleased() async throws {
        let backend = CountingBackend()
        let models = SpeechModels(backend: backend, forceEnergyVAD: true)
        let first = try await models.diarizer()
        let second = try await models.diarizer()
        XCTAssertTrue(first === second)
        XCTAssertEqual(backend.diarizerLoads, 1)

        await models.releaseDiarizer()
        _ = try await models.diarizer()
        XCTAssertEqual(backend.diarizerLoads, 2)
    }

    func testUnloadAlsoFreesTheDiarizer() async throws {
        let backend = CountingBackend()
        let models = SpeechModels(backend: backend, forceEnergyVAD: true)
        _ = try await models.diarizer()

        // End of listening: the diarizer must not survive it (it used to stay for the whole
        // life of the process when a diarization had failed).
        await models.unload()
        _ = try await models.diarizer()
        XCTAssertEqual(backend.diarizerLoads, 2, "unload() must drop the cached diarizer")
    }
}
