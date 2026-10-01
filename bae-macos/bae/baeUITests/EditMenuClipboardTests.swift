import AppKit
import XCTest

/// Edit ▸ Cut, Copy and Paste in the running app act on the focused text, and
/// are enabled only when it can do what they say.
final class EditMenuClipboardTests: XCTestCase {
    @MainActor
    func testClipboardItemsFollowWhatTheFocusedTextCanDo() throws {
        let app = try launchApp()
        // Launch presents the primary window.
        XCTAssertTrue(app.windows.firstMatch.waitForExistence(timeout: 20))
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

        edit.click()
        edit.menuItems["Copy"].click()
        let copied = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in
                NSPasteboard.general.string(forType: .string) == "Album"
            },
            object: nil
        )
        wait(for: [copied], timeout: 5)
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
}
