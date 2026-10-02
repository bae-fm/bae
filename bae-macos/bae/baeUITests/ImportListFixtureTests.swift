import XCTest

/// The import list in the running app opens on the candidates its fixture
/// names, each in the state the fixture gives it, and Found's filter counts
/// them by those states and narrows the list to each entry's.
final class ImportListFixtureTests: XCTestCase {
    /// One candidate folder in each stored state, named for it.
    private static let states: [(String, LibraryFixture.CandidateState)] = [
        ("Not Looked Up Folder", .notLookedUp),
        ("Needs You Folder", .needsYou),
        ("Identified Folder", .identified),
        ("Unmatched Folder", .unmatched),
        ("Lookup Error Folder", .lookupError),
        ("Error Folder", .error),
        ("Import Error Folder", .importError),
    ]

    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testFoundsFilterCountsAndNarrowsToEachStoredState() throws {
        let states = Self.states
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
        // An entry holding no row cannot be chosen.
        XCTAssertFalse(app.menuItems["In Progress (0)"].isEnabled)
        app.typeKey(.escape, modifierFlags: [])

        // Each entry chosen narrows the list to exactly its candidates.
        let entries: [(String, Set<LibraryFixture.CandidateState>)] = [
            (
                "Needs You (4)",
                [.needsYou, .lookupError, .error, .importError]
            ),
            ("Identified (1)", [.identified]),
            ("Unmatched (1)", [.unmatched]),
            ("Not Looked Up (1)", [.notLookedUp]),
            ("All (7)", Set(states.map(\.1))),
        ]
        for (entry, shown) in entries {
            menu.click()
            app.menuItems[entry].click()
            assertListsExactly(shown, in: list, after: entry)
        }
    }

    /// The list shows the folder of each state in `shown`, and of no other.
    @MainActor
    private func assertListsExactly(
        _ shown: Set<LibraryFixture.CandidateState>,
        in list: XCUIElement,
        after entry: String
    ) {
        for (folder, state) in Self.states {
            let row = list.staticTexts[folder]
            if shown.contains(state) {
                XCTAssertTrue(
                    row.waitForExistence(timeout: 10),
                    "\(entry) lists \(folder)"
                )
            }
            else {
                XCTAssertTrue(
                    row.waitForNonExistence(timeout: 10),
                    "\(entry) leaves out \(folder)"
                )
            }
        }
    }
}
