import XCTest

/// A search result picked in the running app shows the album, or the track's
/// row in the album's open detail, wherever the album grid is scrolled: here
/// the album sits a few hundred albums below the top of the grid.
final class SearchRevealTests: XCTestCase {
    /// Albums added after the far one, so it sits below all of them in the
    /// grid's newest-first order: several pages of the grid's list.
    private static let nearCount = 240
    private static let farAlbum = "Distant Album"
    private static let farTracks = (1...12)
        .map {
            String(format: "Distant Song %02d", $0)
        }

    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testPickedResultFarDownTheGridIsScrolledIntoView() throws {
        let near = (0..<Self.nearCount)
            .map { index in
                LibraryFixture.Album(
                    title: String(format: "Album %03d", index),
                    artists: ["Artist"],
                    tracks: [String(format: "Track %03d", index)]
                )
            }
        let app = try launchApp(
            library: LibraryFixture(
                albums: [
                    LibraryFixture.Album(
                        title: Self.farAlbum,
                        artists: ["Artist"],
                        tracks: Self.farTracks
                    )
                ] + near
            )
        )

        // The newest album's card, on top of the grid.
        let newest = near[near.count - 1].title
        let gridView = app.scrollViews
            .containing(.staticText, identifier: newest)
            .firstMatch
        XCTAssertTrue(gridView.waitForExistence(timeout: 30))
        // Its frame now: the query finds it by a card a far scroll takes away.
        let grid = gridView.frame
        XCTAssertFalse(shown(Self.farAlbum, in: grid, of: app))

        pickSearchResult(Self.farAlbum, in: app)
        XCTAssertTrue(
            waitUntil(timeout: 10) {
                self.shown(Self.farAlbum, in: grid, of: app)
            },
            "the picked album is scrolled into view"
        )
        // It is the grid's last: its card goes on top, its detail under it,
        // with no other album's card above it in view.
        XCTAssertTrue(
            waitUntil(timeout: 10) {
                AlbumGrid(app: app).visibleRows(titledFrom: "Album ").isEmpty
            },
            "the picked album's card is on top"
        )

        // Back to the top, with the album's detail left open down there.
        scrollToTop(grid, in: app)
        XCTAssertTrue(
            app.staticTexts[newest].waitForExistence(timeout: 10)
        )
        let farTrack = Self.farTracks[10]
        XCTAssertFalse(shown(farTrack, in: grid, of: app))

        pickSearchResult(farTrack, in: app)
        XCTAssertTrue(
            waitUntil(timeout: 10) {
                self.shown(farTrack, in: grid, of: app)
            },
            "the picked track's row is scrolled into view"
        )
    }

    /// Scroll the grid at `grid` all the way up.
    @MainActor
    private func scrollToTop(_ grid: CGRect, in app: XCUIApplication) {
        let window = app.windows.firstMatch
        window.coordinate(withNormalizedOffset: .zero)
            .withOffset(
                CGVector(
                    dx: grid.midX - window.frame.minX,
                    dy: grid.midY - window.frame.minY
                )
            )
            .scroll(byDeltaX: 0, deltaY: 20_000)
    }

    /// Whether text reading `text` is drawn inside the visible part of
    /// `grid`: an album's title is on its card and atop its open detail.
    @MainActor
    private func shown(
        _ text: String,
        in grid: CGRect,
        of app: XCUIApplication
    ) -> Bool {
        app.staticTexts.matching(identifier: text).allElementsBoundByIndex
            .contains { grid.contains($0.frame) }
    }
}
