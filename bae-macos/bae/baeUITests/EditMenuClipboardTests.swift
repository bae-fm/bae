import AppKit
import XCTest

/// Edit ▸ Cut, Copy and Paste in the running app act on the focused text, and
/// are enabled only when it can do what they say.
final class EditMenuClipboardTests: XCTestCase {
    @MainActor
    func testClipboardItemsFollowWhatTheFocusedTextCanDo() throws {
        let app = try launch()
        let edit = app.menuBars.menuBarItems["Edit"]
        XCTAssertTrue(edit.waitForExistence(timeout: 20))
        // Nothing on the clipboard, so nothing can be pasted.
        NSPasteboard.general.clearContents()

        // The search field, with text in it and none of it selected.
        app.typeKey("/", modifierFlags: [])
        app.typeText("Album")
        XCTAssertEqual(
            enabled(["Cut", "Copy", "Paste"], under: edit, in: app),
            ["Cut": false, "Copy": false, "Paste": false]
        )

        app.typeKey("a", modifierFlags: .command)
        XCTAssertEqual(
            enabled(["Cut", "Copy", "Paste"], under: edit, in: app),
            ["Cut": true, "Copy": true, "Paste": false]
        )

        app.typeKey("c", modifierFlags: .command)
        XCTAssertEqual(NSPasteboard.general.string(forType: .string), "Album")
        XCTAssertEqual(
            enabled(["Paste"], under: edit, in: app),
            ["Paste": true]
        )
    }

    /// Whether each of `titles` is enabled in the Edit menu, read with the
    /// menu open and closed again after.
    @MainActor
    private func enabled(
        _ titles: [String],
        under edit: XCUIElement,
        in app: XCUIApplication
    ) -> [String: Bool] {
        edit.click()
        var states: [String: Bool] = [:]
        for title in titles {
            let item = edit.menuItems[title]
            XCTAssertTrue(item.waitForExistence(timeout: 5), title)
            states[title] = item.isEnabled
        }
        app.typeKey(.escape, modifierFlags: [])
        return states
    }

    /// Launch the app on a library of its own.
    @MainActor
    private func launch() throws -> XCUIApplication {
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: home,
            withIntermediateDirectories: true
        )
        addTeardownBlock {
            try FileManager.default.removeItem(at: home)
        }
        let app = XCUIApplication()
        app.launchEnvironment["HOME"] = home.path
        app.launchEnvironment["BAE_UI_TESTING"] = "1"
        app.launchEnvironment["BAE_UI_TESTING_CREATE_LIBRARY"] = "1"
        app.launch()
        app.activate()
        addTeardownBlock { app.terminate() }
        let window = app.windows.firstMatch
        if !window.waitForExistence(timeout: 2) {
            app.typeKey("n", modifierFlags: .command)
        }
        XCTAssertTrue(window.waitForExistence(timeout: 20))
        return app
    }
}
