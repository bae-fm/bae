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

        let grid = AlbumGrid(app: app)
        // The newest album's card, on top of the grid.
        let newest = near[near.count - 1].title
        XCTAssertTrue(waitUntil(timeout: 30) { grid.shows(newest) })
        XCTAssertFalse(grid.shows(Self.farAlbum))

        pickSearchResult(Self.farAlbum, in: app)
        XCTAssertTrue(
            waitUntil(timeout: 10) { grid.shows(Self.farAlbum) },
            "the picked album is scrolled into view"
        )

        // Back to the top, with the album's detail left open down there.
        scrollToTop(grid)
        XCTAssertTrue(waitUntil(timeout: 10) { grid.shows(newest) })
        let farTrack = Self.farTracks[10]
        XCTAssertFalse(grid.shows(farTrack))

        pickSearchResult(farTrack, in: app)
        XCTAssertTrue(
            waitUntil(timeout: 10) { grid.shows(farTrack) },
            "the picked track's row is scrolled into view"
        )
    }

    /// Scroll the grid all the way up, by twice the height of everything it
    /// lays out, open detail included: the scroll wheel moves the grid less
    /// than the points it is given, and the height grows with the rows a
    /// narrower window wraps the albums into.
    @MainActor
    private func scrollToTop(_ grid: AlbumGrid) {
        let content = grid.scrollView.children(matching: .other).firstMatch
        grid.scroll(by: 2 * content.frame.height)
    }
}
