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

    /// The Edit menu opened between keystrokes in the search field, while
    /// each keystroke's results arrive, offers Select All for the field's
    /// text and Copy once some is selected; chosen, they select and copy all
    /// of it.
    @MainActor
    func testSearchFieldsTextCopiesWhileResultsArrive() throws {
        let albums = LibraryFixture.Album.numbered(300)
        let app = try launchApp(library: LibraryFixture(albums: albums))
        let grid = AlbumGrid(app: app)
        let newest = albums[albums.count - 1].title
        XCTAssertTrue(waitUntil(timeout: 30) { grid.shows(newest) })
        let edit = app.menuBars.menuBarItems["Edit"]
        XCTAssertTrue(edit.waitForExistence(timeout: 20))
        NSPasteboard.general.clearContents()

        let query = "Album 1"
        app.typeKey("/", modifierFlags: [])
        for letter in query {
            app.typeText(String(letter))
            XCTAssertEqual(
                enabled(["Copy", "Select All"], under: edit, in: app),
                ["Copy": false, "Select All": true],
                "after \(letter)"
            )
        }

        edit.click()
        edit.menuItems["Select All"].click()
        edit.click()
        edit.menuItems["Copy"].click()
        let copied = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in
                NSPasteboard.general.string(forType: .string) == query
            },
            object: nil
        )
        wait(for: [copied], timeout: 5)
        // The results for the whole query are listed.
        let result = app.buttons
            .matching(NSPredicate(format: "label BEGINSWITH %@", query))
            .firstMatch
        XCTAssertTrue(result.waitForExistence(timeout: 10))
        XCTAssertEqual(app.state, .runningForeground)
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
