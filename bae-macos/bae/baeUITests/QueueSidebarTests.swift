import XCTest

/// Opening and closing the queue sidebar narrows and widens the album grid,
/// which re-lays its rows at another column count; the grid keeps the album
/// that was on top on top, and goes on answering.
final class QueueSidebarTests: XCTestCase {
    private static let albums = LibraryFixture.Album.numbered(300)

    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testTogglingTheQueueKeepsTheGridsTopAlbum() throws {
        let app = try launchApp(library: LibraryFixture(albums: Self.albums))
        let grid = AlbumGrid(app: app)
        let newest = Self.albums[Self.albums.count - 1].title
        XCTAssertTrue(waitUntil(timeout: 30) { grid.shows(newest) })

        grid.scroll(by: -4_000)
        XCTAssertTrue(waitUntil(timeout: 10) { !grid.shows(newest) })
        let before = grid.visibleRows(titledFrom: "Album ")
        XCTAssertGreaterThanOrEqual(before.count, 2, "\(before)")

        let queueButton = app.windows.firstMatch.buttons["Queue"]
        queueButton.click()
        // The narrower grid has fewer columns, so its top row is shorter.
        var opened: [[String]] = []
        XCTAssertTrue(
            waitUntil(timeout: 10) {
                opened = grid.visibleRows(titledFrom: "Album ")
                return opened.first.map { $0.count < before[0].count }
                    ?? false
            },
            "the grid narrows: \(before) then \(opened)"
        )
        // The album on top before is on top now.
        let held = Set(opened[0])
        XCTAssertFalse(
            held.isDisjoint(with: before[0] + before[1]),
            "\(before) then \(opened)"
        )

        queueButton.click()
        var closed: [[String]] = []
        XCTAssertTrue(
            waitUntil(timeout: 10) {
                closed = grid.visibleRows(titledFrom: "Album ")
                return closed.first?.count == before[0].count
            },
            "the grid widens: \(opened) then \(closed)"
        )
        // Back at the column count it scrolled at, the top row is one it
        // showed then, and holds the album the narrow grid had on top.
        XCTAssertTrue(
            closed[0] == before[0] || closed[0] == before[1],
            "\(before) then \(closed)"
        )
        XCTAssertFalse(
            held.isDisjoint(with: closed[0]),
            "\(opened) then \(closed)"
        )

        // Still answering: a card opens its album's detail.
        let album = closed[0][0]
        grid.scrollView.staticTexts
            .matching(NSPredicate(format: "value == %@", album))
            .firstMatch
            .click()
        XCTAssertTrue(
            waitUntil(timeout: 10) { grid.showsOpen(album) },
            "\(album) opens"
        )
    }
}
