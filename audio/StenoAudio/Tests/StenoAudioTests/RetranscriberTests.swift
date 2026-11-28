@testable import StenoAudioKit
import XCTest

private final class FakeFinal: FinalRecognizer {
    let name = "fake"
    var calls: [Int] = []
    func transcribe(_ samples: [Float]) async throws -> String {
        calls.append(samples.count)
        return samples.count > 40_000 ? "  On passe la TVA belge à 21 %. " : "…"
    }
}

final class RetranscriberTests: XCTestCase {
    func testSliceBornesEtMarge() {
        let s = [Float](repeating: 0, count: 16_000 * 10)
        XCTAssertNil(Retranscriber.slice(s, t0: 1, t1: 1.2), "trop court")
        XCTAssertEqual(Retranscriber.slice(s, t0: 1, t1: 3)?.count, Int(2.3 * 16_000))
        XCTAssertEqual(Retranscriber.slice(s, t0: 9, t1: 12)?.count, Int(1.15 * 16_000), "coupé à la fin du fichier")
        XCTAssertNil(Retranscriber.slice(s, t0: 20, t1: 25), "hors du fichier")
    }

    func testTexteDouteuxRefuse() {
        XCTAssertTrue(Retranscriber.usable("Ce deuxième sujet, c'est qu'on va ouvrir un sujet cross-commun.", duration: 6))
        XCTAssertFalse(Retranscriber.usable("Bonjour, graphique 2.1.1.1.1.1.1.1.1.1.1.1.1.1.1.1.1.1.1.1"))
        XCTAssertFalse(Retranscriber.usable("oui oui oui oui oui oui oui oui oui oui oui oui oui non"))
        XCTAssertFalse(Retranscriber.usable("Une phrase beaucoup trop longue pour une demi-seconde d'audio.", duration: 0.5))
        XCTAssertFalse(Retranscriber.usable(" … "))
        XCTAssertTrue(Retranscriber.usable("D'accord.", duration: 1))
    }

    func testRemplaceSeulementLesTextesExploitables() async {
        let fake = FakeFinal()
        let items = [
            RetranscribeItem(id: 1, file: "them.wav", t0: 0, t1: 3),
            RetranscribeItem(id: 2, file: "them.wav", t0: 4, t1: 5),
            RetranscribeItem(id: 3, file: "me.wav", t0: 0, t1: 0.2),
        ]
        var loads: [String] = []
        let out = await Retranscriber.run(
            items: items, audioDir: URL(fileURLWithPath: "/tmp"), recognizer: fake,
            corrector: VocabularyCorrector(vocabulary: []),
            open: { url in
                loads.append(url.lastPathComponent)
                return InMemoryClipSource(samples: [Float](repeating: 0, count: 16_000 * 6))
            })
        XCTAssertEqual(out, [RetranscribedItem(id: 1, text: "On passe la TVA belge à 21 %.")])
        XCTAssertEqual(loads, ["them.wav", "me.wav"], "chaque fichier lu une seule fois")
        XCTAssertEqual(fake.calls.count, 2, "la phrase trop courte n'est pas envoyée au modèle")
    }

    /// A 10 s WAV written like the sidecar's, where second `k` holds the value `k / 20`.
    private func steppedWav() throws -> (dir: URL, url: URL) {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("steno-audio-tests-\(UUID().uuidString)")
        let url = dir.appendingPathComponent("them.wav")
        let writer = try WavWriter(url: url)
        for k in 0..<10 {
            try writer.append([Float](repeating: Float(k) / 20, count: 16_000))
        }
        try writer.close()
        return (dir, url)
    }

    func testWavExcerptIsReadAtItsBounds() throws {
        let (dir, url) = try steppedWav()
        defer { try? FileManager.default.removeItem(at: dir) }
        let reader = try WavClipReader(url)
        XCTAssertEqual(reader.length, 160_000)
        let range = try XCTUnwrap(Retranscriber.bounds(t0: 3, t1: 5, length: reader.length))
        let chunk = try reader.read(range)
        XCTAssertEqual(chunk.count, Int(2.3 * 16_000))
        XCTAssertEqual(chunk.first ?? -1, 0.10, accuracy: 0.001, "starts at 2.85 s (margin)")
        XCTAssertEqual(chunk[Int(0.15 * 16_000) + 10], 0.15, accuracy: 0.001, "3 s")
        XCTAssertEqual(chunk.last ?? -1, 0.25, accuracy: 0.001, "ends at 5.15 s (margin)")
        let tail = try XCTUnwrap(Retranscriber.bounds(t0: 9, t1: 12, length: reader.length))
        XCTAssertEqual(try reader.read(tail).count, Int(1.15 * 16_000), "cut at the end of the file")
    }

    func testRunReadsEachSentenceFromTheWav() async throws {
        final class Recorder: FinalRecognizer {
            let name = "recorder"
            var chunks: [[Float]] = []
            func transcribe(_ samples: [Float]) async throws -> String {
                chunks.append(samples)
                return "Phrase numéro \(chunks.count)."
            }
        }
        let (dir, _) = try steppedWav()
        defer { try? FileManager.default.removeItem(at: dir) }
        let recorder = Recorder()
        let items = [
            RetranscribeItem(id: 1, file: "them.wav", t0: 1, t1: 3),
            RetranscribeItem(id: 2, file: "them.wav", t0: 7.2, t1: 8.8),
            RetranscribeItem(id: 3, file: "me.wav", t0: 0, t1: 2),
        ]
        let out = await Retranscriber.run(
            items: items, audioDir: dir, recognizer: recorder,
            corrector: VocabularyCorrector(vocabulary: []))
        XCTAssertEqual(out.map(\.id), [1, 2], "missing me.wav: the live text is kept")
        XCTAssertEqual(recorder.chunks.count, 2)
        XCTAssertEqual(Double(recorder.chunks[0].count), 2.3 * 16_000, accuracy: 1)
        XCTAssertEqual(Double(recorder.chunks[1].count), 1.9 * 16_000, accuracy: 1)
        XCTAssertEqual(recorder.chunks[1].first ?? -1, 0.35, accuracy: 0.001, "7.05 s")
        XCTAssertEqual(recorder.chunks[1].last ?? -1, 0.40, accuracy: 0.001, "8.95 s")
    }
}
