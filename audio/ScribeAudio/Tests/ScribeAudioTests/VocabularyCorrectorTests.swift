import XCTest
@testable import ScribeAudioKit

final class VocabularyCorrectorTests: XCTestCase {
    private let corrector = VocabularyCorrector(vocabulary: ["Scribe", "Andromède", "ms-booking", "ORION", "API"])

    func testExactMatchIsCaseAndAccentInsensitive() {
        XCTAssertEqual(corrector.correct("scribe cherche les erreurs"), "Scribe cherche les erreurs")
        XCTAssertEqual(corrector.correct("le projet andromede avance"), "le projet Andromède avance")
        XCTAssertEqual(corrector.correct("daily orion"), "daily ORION")
    }

    func testFuzzyMatchKeepsPunctuation() {
        XCTAssertEqual(corrector.correct("Skribe, cherche les logs."), "Scribe, cherche les logs.")
        XCTAssertEqual(corrector.correct("ok scrybe vérifie"), "ok Scribe vérifie")
    }

    func testMultiWordFormsAreJoined() {
        XCTAssertEqual(corrector.correct("le service MS booking plante"), "le service ms-booking plante")
        XCTAssertEqual(corrector.correct("ms - booking"), "ms-booking")
        XCTAssertEqual(corrector.correct("andro mède"), "Andromède")
    }

    func testWindowDoesNotCrossPunctuation() {
        XCTAssertEqual(corrector.correct("ms, booking"), "ms, booking")
    }

    func testTrapWordsAreNeverReplaced() {
        // Mots pièges du plan (§6.2) : jamais transformés en mot d'appel.
        for text in ["le script de déploiement", "la description du ticket", "il s'est inscrit hier",
                     "prescrire une solution", "les scripts tournent", "un scribouillard"] {
            XCTAssertEqual(corrector.correct(text), text, text)
        }
    }

    func testShortEntriesAreIgnored() {
        // « API » (3 lettres) est ignoré : pas de risque sur « j'ai », « api »…
        XCTAssertEqual(corrector.correct("j'ai vu l'api"), "j'ai vu l'api")
    }

    func testAlreadyCanonicalTextIsUnchanged() {
        let text = "Scribe, cherche les erreurs Datadog sur ms-booking depuis hier."
        XCTAssertEqual(corrector.correct(text), text)
    }

    func testEmptyVocabulary() {
        let empty = VocabularyCorrector(vocabulary: [])
        XCTAssertTrue(empty.isEmpty)
        XCTAssertEqual(empty.correct("scribe"), "scribe")
    }

    func testNormalizeAndDistance() {
        XCTAssertEqual(VocabularyCorrector.normalize("Ms-Booking"), "msbooking")
        XCTAssertEqual(VocabularyCorrector.normalize("Éléonore"), "eleonore")
        XCTAssertEqual(VocabularyCorrector.levenshtein(Array("scribe"), Array("script"), limit: 5), 2)
        XCTAssertEqual(VocabularyCorrector.levenshtein(Array("scribe"), Array("skribe"), limit: 5), 1)
        XCTAssertEqual(VocabularyCorrector.levenshtein(Array("abc"), Array("xyzuvw"), limit: 1), 2, "borne dépassée → limit + 1")
        XCTAssertEqual(VocabularyCorrector.maxDistance(forLength: 4), 0)
        XCTAssertEqual(VocabularyCorrector.maxDistance(forLength: 6), 1)
        XCTAssertEqual(VocabularyCorrector.maxDistance(forLength: 9), 2)
    }
}
