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
            load: { url in
                loads.append(url.lastPathComponent)
                return [Float](repeating: 0, count: 16_000 * 6)
            })
        XCTAssertEqual(out, [RetranscribedItem(id: 1, text: "On passe la TVA belge à 21 %.")])
        XCTAssertEqual(loads, ["them.wav", "me.wav"], "chaque fichier lu une seule fois")
        XCTAssertEqual(fake.calls.count, 2, "la phrase trop courte n'est pas envoyée au modèle")
    }
}
