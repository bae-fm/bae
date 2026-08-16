import XCTest

final class MainMenuTests: XCTestCase {
    @MainActor
    func testLibraryShortcutKeepsTheFocusedMainWindow() throws {
        try assertSectionShortcutKeepsTheFocusedMainWindow("1")
    }

    @MainActor
    func testImportShortcutKeepsTheFocusedMainWindow() throws {
        try assertSectionShortcutKeepsTheFocusedMainWindow("2")
    }

    @MainActor
    func testCloseLibraryCommandReturnsToTheWelcomeChooser() throws {
        let testHome = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: testHome,
            withIntermediateDirectories: true
        )
        addTeardownBlock {
            try FileManager.default.removeItem(at: testHome)
        }

        let app = XCUIApplication()
        app.launchEnvironment["HOME"] = testHome.path
        app.launchEnvironment["BAE_UI_TESTING"] = "1"
        app.launchEnvironment["BAE_UI_TESTING_CREATE_LIBRARY"] = "1"
        app.launch()
        app.activate()
        addTeardownBlock { app.terminate() }

        let primaryWindow = app.windows.firstMatch
        if !primaryWindow.waitForExistence(timeout: 2) {
            app.typeKey("n", modifierFlags: .command)
        }
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

    @MainActor
    private func assertSectionShortcutKeepsTheFocusedMainWindow(
        _ key: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) throws {
        let testHome = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: testHome,
            withIntermediateDirectories: true
        )
        addTeardownBlock {
            try FileManager.default.removeItem(at: testHome)
        }

        let app = XCUIApplication()
        app.launchEnvironment["HOME"] = testHome.path
        app.launchEnvironment["BAE_UI_TESTING"] = "1"
        app.launchEnvironment["BAE_UI_TESTING_CREATE_LIBRARY"] = "1"
        app.launch()
        app.activate()
        addTeardownBlock { app.terminate() }

        let primaryWindow = app.windows.firstMatch
        if !primaryWindow.waitForExistence(timeout: 2) {
            app.typeKey("n", modifierFlags: .command)
        }
        XCTAssertTrue(
            primaryWindow.waitForExistence(timeout: 20),
            file: file,
            line: line
        )
        XCTAssertEqual(app.windows.count, 1, file: file, line: line)

        app.typeKey(key, modifierFlags: .command)

        XCTAssertFalse(
            app.windows.element(boundBy: 1).waitForExistence(timeout: 2),
            file: file,
            line: line
        )
        XCTAssertEqual(app.windows.count, 1, file: file, line: line)
    }
}
