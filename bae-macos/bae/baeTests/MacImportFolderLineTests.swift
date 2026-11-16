import XCTest

@testable import bae

/// The folder line names the queue tab the folder is on, in a chip a folder
/// on no tab does not draw. How the chip looks is the import-metadata-draft
/// and import-pane-queued shots'.
final class MacImportFolderLineTests: XCTestCase {
    @MainActor
    func testHeaderStatesCurrentPlacement() {
        XCTAssertEqual(MacImportFolderLine.label(for: .pending), "Found")
        XCTAssertEqual(MacImportFolderLine.label(for: .done), "Imported")
        XCTAssertEqual(MacImportFolderLine.label(for: .skipped), "Skipped")
    }
}
