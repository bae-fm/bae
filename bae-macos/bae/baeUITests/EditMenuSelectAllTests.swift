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

        // The section switch, which the window shows once the library is
        // open, not while it opens it.
        let importSection = app.windows.firstMatch.sectionTab("Import")
        XCTAssertTrue(importSection.waitForExistence(timeout: 20))
        importSection.click()
        let list = app.tables["import-list"]
        // The list draws only the rows on screen, and the count it reaches
        // with Select All is the check that it lists every candidate.
        XCTAssertTrue(
            list.staticTexts["Album 000"].waitForExistence(timeout: 20)
        )
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

        // One row selected again: the import screen shows its filter field
        // beside the list, which it does not with several rows selected in a
        // window as narrow as the CI runner's.
        topRow.click()
        XCTAssertTrue(selectedAll.waitForNonExistence(timeout: 10))
        let filter = app.searchFields
            .matching(
                NSPredicate(format: "placeholderValue == %@", "Filter")
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
