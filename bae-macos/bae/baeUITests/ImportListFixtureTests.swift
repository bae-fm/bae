import XCTest

/// The import list in the running app opens on the candidates its fixture
/// names, each in the state the fixture gives it, and Found's filter counts
/// them by those states.
final class ImportListFixtureTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testFoundsFilterCountsEachStoredState() throws {
        let states: [(String, LibraryFixture.CandidateState)] = [
            ("Not Looked Up Folder", .notLookedUp),
            ("Needs You Folder", .needsYou),
            ("Identified Folder", .identified),
            ("Unmatched Folder", .unmatched),
            ("Lookup Error Folder", .lookupError),
            ("Error Folder", .error),
            ("Import Error Folder", .importError),
        ]
        let app = try launchApp(
            library: LibraryFixture(
                watchedFolders: [
                    LibraryFixture.WatchedFolder(
                        name: "music",
                        candidates: states.map { folder, state in
                            LibraryFixture.Candidate(
                                folder: folder,
                                tracks: ["01 Track.flac", "02 Track.flac"],
                                state: state
                            )
                        }
                    )
                ]
            )
        )

        let window = app.windows.firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 20))
        window.buttons["Import"].click()
        let list = app.outlines.firstMatch
        for (folder, _) in states {
            XCTAssertTrue(
                list.staticTexts[folder].waitForExistence(timeout: 20),
                folder
            )
        }

        // The list's menu: its trigger is the "More" symbol.
        let menu = window.menuButtons["More"]
        XCTAssertTrue(menu.waitForExistence(timeout: 10))
        menu.click()
        // Lookup Error, Error and Import Error wait on the person as Needs
        // You does, so its entry holds all four.
        for entry in [
            "All (7)", "Needs You (4)", "In Progress (0)", "Identified (1)",
            "Unmatched (1)", "Not Looked Up (1)",
        ] {
            XCTAssertTrue(
                app.menuItems[entry].waitForExistence(timeout: 10),
                entry
            )
        }
        app.typeKey(.escape, modifierFlags: [])
    }
}
