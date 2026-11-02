import XCTest

@testable import bae

/// The folder line names the queue tab the folder is on, in a chip a folder
/// on no tab does not draw. How the chip looks is the import-metadata-draft
/// and import-pane-queued shots'.
final class CandidateFolderLineTests: XCTestCase {
    @MainActor
    func testHeaderStatesCurrentPlacement() {
        XCTAssertEqual(CandidateFolderLine.label(for: .pending), "Found")
        XCTAssertEqual(CandidateFolderLine.label(for: .done), "Imported")
        XCTAssertEqual(CandidateFolderLine.label(for: .skipped), "Skipped")
    }
}
