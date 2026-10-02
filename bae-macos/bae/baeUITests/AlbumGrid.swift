import XCTest

/// The running app's album grid: what it draws inside the part of it on
/// screen. An album's title is on its card and atop its open detail; a
/// track's title is on its row in its album's open detail.
@MainActor
struct AlbumGrid {
    let app: XCUIApplication

    /// The grid's scroll view, which the library mounts only while it shows
    /// its albums.
    var scrollView: XCUIElement {
        app.scrollViews["album-grid"]
    }

    /// Where each text reading `text` is drawn wholly inside the grid's
    /// visible area, top to bottom.
    func frames(of text: String) -> [CGRect] {
        let visible = scrollView.frame
        return scrollView.staticTexts
            .matching(NSPredicate(format: "value == %@", text))
            .allElementsBoundByIndex
            .map(\.frame)
            .filter { visible.contains($0) }
            .sorted { $0.minY < $1.minY }
    }

    /// Whether text reading `text` is drawn inside the grid's visible area.
    func shows(_ text: String) -> Bool {
        !frames(of: text).isEmpty
    }

    /// Whether `album`'s detail is open with its card: its title shows on
    /// both.
    func showsOpen(_ album: String) -> Bool {
        frames(of: album).count >= 2
    }
}

extension XCTestCase {
    /// Whether `condition` holds within `timeout`, checked as the app runs.
    @MainActor
    func waitUntil(
        timeout: TimeInterval,
        _ condition: @escaping () -> Bool
    ) -> Bool {
        let expectation = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in condition() },
            object: nil
        )
        return XCTWaiter.wait(for: [expectation], timeout: timeout)
            == .completed
    }

    /// Search for `title` in the title bar and pick the result it titles.
    @MainActor
    func pickSearchResult(_ title: String, in app: XCUIApplication) {
        let field = app.textFields
            .matching(NSPredicate(format: "placeholderValue == %@", "Search"))
            .firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        field.click()
        field.typeText(title)
        // A result's label is its title, then what its subtitle says.
        let result = app.buttons
            .matching(NSPredicate(format: "label BEGINSWITH %@", "\(title),"))
            .firstMatch
        XCTAssertTrue(result.waitForExistence(timeout: 10), title)
        result.click()
    }
}

extension LibraryFixture.Album {
    /// `count` albums titled `Album 000` on, each of one track titled
    /// `Track 000` on, all by one artist.
    static func numbered(_ count: Int) -> [Self] {
        (0..<count)
            .map { index in
                Self(
                    title: String(format: "Album %03d", index),
                    artists: ["Artist"],
                    tracks: [String(format: "Track %03d", index)]
                )
            }
    }
}
