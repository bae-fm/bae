import XCTest

final class MainMenuTests: XCTestCase {
    @MainActor
    func testOpenLibraryWindowExposesCloseLibraryCommand() throws {
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

        let fileMenu = app.menuBars.menuBarItems["File"]
        XCTAssertTrue(fileMenu.waitForExistence(timeout: 20))
        fileMenu.click()
        let closeLibrary = app.menuItems["Close Library"]
        XCTAssertTrue(closeLibrary.exists)
        closeLibrary.click()

        XCTAssertEqual(app.state, .runningForeground)
        XCTAssertFalse(fileMenu.exists)
    }
}
