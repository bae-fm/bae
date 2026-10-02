import XCTest

/// A track picked from the search results is shown in its album's open
/// detail, wherever the album is in the grid: its row scrolled into view.
/// The near album is on the grid's first screen; the far one sits a few
/// hundred albums below it.
final class SearchTrackPickTests: XCTestCase {
    /// Albums added after the far one, so it sits below all of them in the
    /// grid's newest-first order: several pages of the grid's list.
    private static let nearCount = 300
    private static let farAlbum = "Distant Album"
    /// More tracks than the far album's detail shows in one screen, so its
    /// late tracks' rows are below its card's.
    private static let farTracks = (1...12)
        .map {
            String(format: "Distant Song %02d", $0)
        }
    private static let near = LibraryFixture.Album.numbered(nearCount)
    /// Albums added before the far one, so it is not in the grid's last
    /// row: the open album's detail shows below its card.
    private static let older = (0..<10)
        .map { index in
            LibraryFixture.Album(
                title: String(format: "Older Album %02d", index),
                artists: ["Artist"],
                tracks: [String(format: "Older Track %02d", index)]
            )
        }
    /// The newest album's, on top of the grid.
    private static let newest = near[nearCount - 1].title
    /// A track of an album in the grid's top row.
    private static let nearTrack = near[nearCount - 2].tracks[0]

    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testPickedTrackIsShownInItsOpenAlbumNearAndFar() throws {
        let app = try launchOnTheGrid()
        let grid = AlbumGrid(app: app)

        pickSearchResult(Self.nearTrack, in: app)
        XCTAssertTrue(
            waitUntil(timeout: 10) { grid.shows(Self.nearTrack) },
            "the near track's row is in view"
        )
        XCTAssertTrue(grid.shows(Self.newest), "the grid stays at its top")

        // Its album closed and far below.
        let farTrack = Self.farTracks[10]
        XCTAssertFalse(grid.shows(farTrack))
        pickSearchResult(farTrack, in: app)
        XCTAssertTrue(
            waitUntil(timeout: 10) { grid.shows(farTrack) },
            "the far track's row is scrolled into view"
        )

        // Back to the top, with the album's detail left open down there.
        grid.scroll(by: 20_000)
        XCTAssertTrue(waitUntil(timeout: 10) { grid.shows(Self.newest) })
        let otherFarTrack = Self.farTracks[11]
        XCTAssertFalse(grid.shows(otherFarTrack))
        pickSearchResult(otherFarTrack, in: app)
        XCTAssertTrue(
            waitUntil(timeout: 10) { grid.shows(otherFarTrack) },
            "the open album's far track's row is scrolled into view"
        )
    }

    /// Launch on the far album between the older albums and the near ones,
    /// once the grid shows its newest album.
    @MainActor
    private func launchOnTheGrid() throws -> XCUIApplication {
        let far = LibraryFixture.Album(
            title: Self.farAlbum,
            artists: ["Artist"],
            tracks: Self.farTracks
        )
        let app = try launchApp(
            library: LibraryFixture(albums: Self.older + [far] + Self.near)
        )
        let grid = AlbumGrid(app: app)
        XCTAssertTrue(
            waitUntil(timeout: 30) { grid.shows(Self.newest) },
            "the grid opens on its newest album"
        )
        XCTAssertFalse(grid.shows(Self.farAlbum))
        return app
    }
}
