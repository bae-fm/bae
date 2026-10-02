import XCTest

/// Edit ▸ Select All and Command-A in the running app: in the import list they
/// select every row the list shows, past the rows its pages have loaded, and
/// in a text field they select the field's text.
final class EditMenuSelectAllTests: XCTestCase {
    /// More rows than the list loads in one page.
    private static let albumCount = 120

    @MainActor
    func testSelectAllReachesEveryListedRowAndStillSelectsText() throws {
        let app = try launchApp(
            library: LibraryFixture(
                watchedFolders: [
                    LibraryFixture.WatchedFolder(
                        name: "music",
                        candidates: (0..<Self.albumCount)
                            .map { index in
                                LibraryFixture.Candidate(
                                    folder: String(format: "Album %03d", index),
                                    tracks: ["01 Track.flac"],
                                    state: .notLookedUp
                                )
                            }
                    )
                ]
            )
        )
        let selectedAll = app.staticTexts["\(Self.albumCount) selected"]

        let window = app.windows.firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 20))
        window.buttons["Import"].click()
        let list = app.outlines.firstMatch
        // The first and the last of the folder's candidates: then every one is
        // in the list, whichever order it lists them in.
        for album in ["Album 000", "Album \(Self.albumCount - 1)"] {
            XCTAssertTrue(list.staticTexts[album].waitForExistence(timeout: 20))
        }
        // The top row, wherever the list's order put it.
        let topRow = list.coordinate(withNormalizedOffset: .zero)
            .withOffset(CGVector(dx: 120, dy: 30))

        topRow.click()
        app.typeKey("a", modifierFlags: .command)
        XCTAssertTrue(selectedAll.waitForExistence(timeout: 10))

        // One row selected: the all-rows selection is gone.
        topRow.click()
        XCTAssertTrue(selectedAll.waitForNonExistence(timeout: 10))
        app.menuBars.menuBarItems["Edit"].click()
        app.menuBars.menuItems["Select All"].click()
        XCTAssertTrue(selectedAll.waitForExistence(timeout: 10))

        let filter = app.textFields
            .matching(
                NSPredicate(format: "placeholderValue == %@", "Filter...")
            )
            .firstMatch
        XCTAssertTrue(filter.waitForExistence(timeout: 10))
        filter.click()
        filter.typeText("Album")
        app.typeKey("a", modifierFlags: .command)
        filter.typeText("Z")
        XCTAssertEqual(filter.value as? String, "Z")
    }
}
