@testable import StenoAudioKit
import XCTest

final class HallucinationFilterTests: XCTestCase {
    private func reject(_ text: String, confidence: Float? = 0.9, duration: Double = 0.8, db: Double = -30) -> Bool {
        HallucinationFilter.rejection(text: text, confidence: confidence, duration: duration, rmsDb: db) != nil
    }

    func testInterjectionsAnglaisesCourtesEcartees() {
        XCTAssertTrue(reject("Shall we?"))
        XCTAssertTrue(reject("Thank God."))
        XCTAssertTrue(reject("Oh."))
        XCTAssertTrue(reject("Okay."))
        XCTAssertTrue(reject("Mm-hmm."))
        // Longue mais peu fiable.
        XCTAssertTrue(reject("Thank you.", confidence: 0.5, duration: 2.5))
    }

    func testInterjectionLongueEtFiableConservee() {
        XCTAssertFalse(reject("Okay.", confidence: 0.95, duration: 2.0))
    }

    func testFrancaisCourtConserve() {
        XCTAssertFalse(reject("Oui."))
        XCTAssertFalse(reject("D'accord."))
        XCTAssertFalse(reject("Je regarde ça."))
    }

    func testPhraseNormaleConservee() {
        XCTAssertFalse(reject("Point avec Baptiste pour faire le point sur la mise en prod.", confidence: 0.4, duration: 4))
    }

    func testSilenceEcarte() {
        XCTAssertTrue(reject("Bonjour à tous.", db: -60))
    }

    func testPhraseCourteTresPeuFiableEcartee() {
        XCTAssertTrue(reject("Les tickets", confidence: 0.2))
        XCTAssertFalse(reject("Les tickets", confidence: 0.6))
        // Sans confiance connue : seule la règle des interjections s'applique.
        XCTAssertFalse(reject("Les tickets", confidence: nil))
    }

    func testNiveau() {
        XCTAssertLessThan(HallucinationFilter.rmsDb([Float](repeating: 0, count: 100)), -100)
        XCTAssertEqual(HallucinationFilter.rmsDb([Float](repeating: 0.1, count: 100)), -20, accuracy: 0.01)
    }
}
