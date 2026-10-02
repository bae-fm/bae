import XCTest

/// The album grid's sort is a preference: the app keeps it in its defaults,
/// which under a UI test are the test's own.
final class LibrarySortTests: XCTestCase {
    private static let albums = LibraryFixture.Album.numbered(3)

    override func setUp() {
        continueAfterFailure = false
    }

    /// Flipping the sort's direction puts the oldest album first and keeps
    /// that sort in the app's defaults.
    @MainActor
    func testFlippingTheSortKeepsItInTheAppsDefaults() throws {
        let app = try launchApp(library: LibraryFixture(albums: Self.albums))
        let grid = AlbumGrid(app: app)
        // Newest added first: the last album added leads.
        XCTAssertTrue(
            waitUntil(timeout: 30) {
                self.leadingAlbum(in: grid) == Self.albums[2].title
            }
        )

        app.buttons["Sort Ascending"].firstMatch.click()

        XCTAssertTrue(
            waitUntil(timeout: 10) {
                self.leadingAlbum(in: grid) == Self.albums[0].title
            },
            "the oldest album leads"
        )
        XCTAssertTrue(
            waitUntil(timeout: 10) {
                self.storedSort(of: app)
                    == [["field": "dateAdded", "direction": "ascending"]]
            },
            "the app keeps the sort in its defaults"
        )
    }

    /// The first album the grid shows.
    @MainActor
    private func leadingAlbum(in grid: AlbumGrid) -> String? {
        grid.visibleRows(titledFrom: "Album ").first?.first
    }

    /// The album sort the app's defaults hold, as the JSON it stores.
    @MainActor
    private func storedSort(of app: XCUIApplication) -> [[String: String]]? {
        guard let data = app.appDefaults.data(forKey: "librarySortCriteria")
        else {
            return nil
        }
        return (try? JSONSerialization.jsonObject(with: data))
            as? [[String: String]]
    }
}
