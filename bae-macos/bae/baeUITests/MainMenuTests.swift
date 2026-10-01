import XCTest

final class MainMenuTests: XCTestCase {
    @MainActor
    func testLibraryShortcutKeepsTheFocusedMainWindow() throws {
        try assertSectionShortcutKeepsTheFocusedMainWindow(
            "1",
            from: .importing,
            to: .library
        )
    }

    @MainActor
    func testImportShortcutKeepsTheFocusedMainWindow() throws {
        try assertSectionShortcutKeepsTheFocusedMainWindow(
            "2",
            from: .library,
            to: .importing
        )
    }

    @MainActor
    func testCloseLibraryCommandReturnsToTheWelcomeChooser() throws {
        let app = try launchApp()

        // Launch presents the primary window.
        let primaryWindow = app.windows.firstMatch
        XCTAssertTrue(primaryWindow.waitForExistence(timeout: 20))
        XCTAssertEqual(primaryWindow.frame.width, 1_350, accuracy: 2)

        let fileMenu = app.menuBars.menuBarItems["File"]
        XCTAssertTrue(fileMenu.waitForExistence(timeout: 20))
        let editMenu = app.menuBars.menuBarItems["Edit"]
        XCTAssertTrue(editMenu.exists)
        XCTAssertLessThan(fileMenu.frame.minX, editMenu.frame.minX)
        fileMenu.click()
        let closeLibrary = app.menuItems["Close Library"]
        XCTAssertTrue(closeLibrary.exists)
        closeLibrary.coordinate(
            withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)
        )
        .click()

        XCTAssertEqual(app.state, .runningForeground)
        XCTAssertTrue(
            app.staticTexts["Get started with your music library."]
                .waitForExistence(timeout: 20)
        )
        XCTAssertEqual(primaryWindow.frame.width, 900, accuracy: 2)
    }

    /// A main-window section of the empty library a test launches on: the
    /// title-bar segment that selects it, and the text only it shows.
    private struct Section {
        let segment: String
        let shows: String

        static let library = Section(segment: "Library", shows: "No albums")
        static let importing = Section(
            segment: "Import",
            shows: "Add a folder to import music from"
        )
    }

    /// Start on `start`, press Command-`key`, and once `target` shows, the
    /// window that showed it is still the only one.
    @MainActor
    private func assertSectionShortcutKeepsTheFocusedMainWindow(
        _ key: String,
        from start: Section,
        to target: Section,
        file: StaticString = #filePath,
        line: UInt = #line
    ) throws {
        let app = try launchApp()

        // Launch presents the primary window.
        let primaryWindow = app.windows.firstMatch
        XCTAssertTrue(
            primaryWindow.waitForExistence(timeout: 20),
            file: file,
            line: line
        )
        XCTAssertEqual(app.windows.count, 1, file: file, line: line)

        primaryWindow.buttons[start.segment].click()
        XCTAssertTrue(
            primaryWindow.staticTexts[start.shows]
                .waitForExistence(timeout: 20),
            file: file,
            line: line
        )

        app.typeKey(key, modifierFlags: .command)

        XCTAssertTrue(
            app.staticTexts[target.shows].waitForExistence(timeout: 20),
            file: file,
            line: line
        )
        XCTAssertEqual(app.windows.count, 1, file: file, line: line)
    }
}
