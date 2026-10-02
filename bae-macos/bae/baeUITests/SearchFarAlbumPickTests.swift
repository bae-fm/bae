import XCTest

/// An album picked from the search results opens the library on it from the
/// import section, wherever it is in the album grid: scrolled into view with
/// its detail open. The near album is on the grid's first screen; the far one
/// sits a few hundred albums below it.
final class SearchFarAlbumPickTests: XCTestCase {
    /// Albums added after the far one, so it sits below all of them in the
    /// grid's newest-first order: several pages of the grid's list.
    private static let nearCount = 300
    private static let farAlbum = "Distant Album"
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
    /// An album of the grid's top row.
    private static let nearAlbum = near[nearCount - 2].title

    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testPickedAlbumOpensInTheLibraryNearAndFar() throws {
        let far = LibraryFixture.Album(
            title: Self.farAlbum,
            artists: ["Artist"],
            tracks: Self.farTracks
        )
        let app = try launchApp(
            library: LibraryFixture(albums: Self.older + [far] + Self.near)
        )
        let grid = AlbumGrid(app: app)
        let window = app.windows.firstMatch
        XCTAssertTrue(
            waitUntil(timeout: 30) { grid.shows(Self.newest) },
            "the grid opens on its newest album"
        )

        // Each pick is made from Import, which unmounts the library.
        for (album, near) in [(Self.nearAlbum, true), (Self.farAlbum, false)] {
            window.buttons["Import"].click()
            XCTAssertTrue(grid.scrollView.waitForNonExistence(timeout: 10))
            pickSearchResult(album, in: app)
            XCTAssertTrue(
                waitUntil(timeout: 10) { grid.showsOpen(album) },
                "\(album) is in view, open"
            )
            XCTAssertTrue(window.buttons["Library"].isSelected)
            // The near album opens where the grid opened; the far one is
            // scrolled to.
            XCTAssertEqual(grid.shows(Self.newest), near, album)
        }
    }
}
