@testable import StenoAudioKit
import XCTest

/// `STENO_MODELS_DIR` layout: every model gets its own subfolder of the same base folder.
/// Nothing is downloaded here, only paths are checked.
final class ModelLocationsTests: XCTestCase {
    private let base = URL(fileURLWithPath: "/models", isDirectory: true)

    func testEveryModelHasItsOwnSubfolderOfTheBase() {
        let locations = ModelLocations(base: base)
        let folders = [locations.parakeet, locations.vad, locations.diarizer, locations.canary]
        XCTAssertEqual(folders.map(\.lastPathComponent), ["parakeet-tdt-v3", "silero-vad", "diarizer", "canary-1b-v2"])
        for folder in folders {
            XCTAssertEqual(folder.deletingLastPathComponent().path, base.path, folder.path)
        }
    }

    func testParakeetKeepsTheFolderUsedBeforeTheOtherModelsFollowedIt() {
        // Folders already prepared with STENO_MODELS_DIR must not download Parakeet again.
        XCTAssertEqual(ModelLocations(base: base).parakeet.path, "/models/parakeet-tdt-v3")
    }

    func testCanaryModelsSitInTheRepoFolderUnderTheirDownloadRoot() {
        // `ModelHub.download(.canary1bV2, to:)` writes into `<root>/<Repo.folderName>`. A new
        // FluidAudio folder name would make prepared folders download Canary again: this test
        // flags it.
        let locations = ModelLocations(base: base)
        XCTAssertEqual(locations.canaryModels.path, "/models/canary-1b-v2/canary-1b-v2")
    }
}
