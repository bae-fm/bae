import XCTest

/// The running app's album grid: what it draws inside the part of it on
/// screen, and scrolling it. An album's title is on its card and atop its
/// open detail; a track's title is on its row in its album's open detail.
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
        return laidOutFrames(of: text).filter { visible.contains($0) }
    }

    /// Where each text reading `text` is laid out in the grid, in view or
    /// scrolled out of it, top to bottom: the grid lays out the rows near
    /// the ones in view too.
    func laidOutFrames(of text: String) -> [CGRect] {
        scrollView.staticTexts
            .matching(NSPredicate(format: "value == %@", text))
            .allElementsBoundByIndex
            .map(\.frame)
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

    /// The titles of the album cards drawn wholly inside the visible area,
    /// row by row from the top, each row left to right. Every title begins
    /// with `prefix`.
    func visibleRows(titledFrom prefix: String) -> [[String]] {
        let visible = scrollView.frame
        let titles = scrollView.staticTexts
            .matching(NSPredicate(format: "value BEGINSWITH %@", prefix))
            .allElementsBoundByIndex
            .map { (text: $0.value as? String ?? "", frame: $0.frame) }
            .filter { visible.contains($0.frame) }
            .sorted {
                ($0.frame.minY, $0.frame.minX) < ($1.frame.minY, $1.frame.minX)
            }
        var rows: [(minY: CGFloat, titles: [String])] = []
        for title in titles {
            if let last = rows.last, abs(last.minY - title.frame.minY) < 2 {
                rows[rows.count - 1].titles.append(title.text)
            }
            else {
                rows.append((title.frame.minY, [title.text]))
            }
        }
        return rows.map(\.titles)
    }

    /// Scroll the grid by `deltaY` points: down when negative.
    func scroll(by deltaY: CGFloat) {
        scrollView.scroll(byDeltaX: 0, deltaY: deltaY)
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
