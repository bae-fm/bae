import XCTest

/// An album picked from the search results while the import section shows
/// opens the library on the album, with its detail open where the grid
/// shows it.
final class SearchAlbumPickTests: XCTestCase {
    private static let albums = LibraryFixture.Album.numbered(40)
    /// The newest album's, on top of the grid.
    private static let newest = albums[albums.count - 1].title
    /// An album of the grid's top row.
    private static let picked = albums[albums.count - 2].title

    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testPickedAlbumOpensInTheLibraryFromImport() throws {
        let app = try launchApp(library: LibraryFixture(albums: Self.albums))
        let grid = AlbumGrid(app: app)
        let window = app.windows.firstMatch
        XCTAssertTrue(
            waitUntil(timeout: 30) { grid.shows(Self.newest) },
            "the grid opens on its newest album"
        )

        // Import unmounts the library.
        window.buttons["Import"].click()
        XCTAssertTrue(grid.scrollView.waitForNonExistence(timeout: 10))
        pickSearchResult(Self.picked, in: app)
        XCTAssertTrue(
            waitUntil(timeout: 10) { grid.showsOpen(Self.picked) },
            "the album is in view, open"
        )
        XCTAssertTrue(window.buttons["Library"].isSelected)
        XCTAssertTrue(grid.shows(Self.newest), "the grid stays at its top")
    }
}
