import XCTest

/// The album grid opens one album's detail at a time, under that album's
/// row. Opening another album of the same row shows it in the detail where
/// the detail is; opening one of another row moves the detail under that
/// row.
final class AlbumDetailSwapTests: XCTestCase {
    /// Three rows' worth at the grid's widest, each album with tracks of its
    /// own.
    private static let albums = (0..<12)
        .map { index in
            LibraryFixture.Album(
                title: String(format: "Album %02d", index),
                artists: ["Artist"],
                tracks: (1...3)
                    .map {
                        String(format: "Album %02d Track %d", index, $0)
                    }
            )
        }

    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testAnotherAlbumOpensInPlaceInItsRowAndMovesTheDetailToAnother()
        throws
    {
        let app = try launchApp(library: LibraryFixture(albums: Self.albums))
        let grid = AlbumGrid(app: app)
        let newest = Self.albums[Self.albums.count - 1].title
        XCTAssertTrue(waitUntil(timeout: 30) { grid.shows(newest) })
        let rows = grid.visibleRows(titledFrom: "Album ")
        XCTAssertGreaterThanOrEqual(rows.count, 2, "\(rows)")
        XCTAssertGreaterThanOrEqual(rows[0].count, 2, "\(rows)")
        let first = rows[0][0]
        let sameRow = rows[0][rows[0].count - 1]
        let nextRow = rows[1][0]
        // How far one row of cards is below the one above it.
        let firstTop = try XCTUnwrap(grid.frames(of: first).first).minY
        let nextTop = try XCTUnwrap(grid.frames(of: nextRow).first).minY
        let rowPitch = nextTop - firstTop

        open(first, in: grid)
        let firstCard = try XCTUnwrap(grid.laidOutFrames(of: first).first)
        let detail = try XCTUnwrap(grid.laidOutFrames(of: first).last)

        open(sameRow, in: grid)
        XCTAssertTrue(
            waitUntil(timeout: 10) { grid.laidOutFrames(of: first).count == 1 },
            "\(first) closes as \(sameRow) opens"
        )
        XCTAssertTrue(grid.laidOutFrames(of: track(1, of: first)).isEmpty)
        // The detail shows the other album where it showed the first, under
        // the same row, which stays where it was.
        XCTAssertEqual(grid.laidOutFrames(of: sameRow).last?.minY, detail.minY)
        XCTAssertEqual(
            grid.laidOutFrames(of: first).first?.minY,
            firstCard.minY
        )

        // The next row is under the detail; scroll it into view, past the
        // header the scroll folds away.
        grid.scroll(by: -2 * rowPitch)
        XCTAssertTrue(waitUntil(timeout: 10) { grid.shows(nextRow) })
        open(nextRow, in: grid)
        XCTAssertTrue(
            waitUntil(timeout: 10) {
                grid.laidOutFrames(of: sameRow).count == 1
            },
            "\(sameRow) closes as \(nextRow) opens"
        )
        XCTAssertTrue(grid.laidOutFrames(of: track(1, of: sameRow)).isEmpty)
        // The detail left the first row, so the next row sits right under
        // it again, and the detail is under the next row.
        let sameRowCard = try XCTUnwrap(grid.laidOutFrames(of: sameRow).first)
        let nextCard = try XCTUnwrap(grid.laidOutFrames(of: nextRow).first)
        let nextDetail = try XCTUnwrap(grid.laidOutFrames(of: nextRow).last)
        XCTAssertEqual(
            nextCard.minY - sameRowCard.minY,
            rowPitch,
            accuracy: 1
        )
        XCTAssertGreaterThan(nextDetail.minY, nextCard.maxY)
    }

    /// Click `album`'s card, and wait for its detail to lay out its title and
    /// tracks.
    @MainActor
    private func open(_ album: String, in grid: AlbumGrid) {
        grid.scrollView.staticTexts
            .matching(NSPredicate(format: "value == %@", album))
            .firstMatch
            .click()
        XCTAssertTrue(
            waitUntil(timeout: 10) {
                grid.laidOutFrames(of: album).count == 2
                    && !grid.laidOutFrames(of: self.track(1, of: album))
                        .isEmpty
            },
            "\(album) opens"
        )
    }

    /// The title of `album`'s track `number`, from 1.
    private func track(_ number: Int, of album: String) -> String {
        "\(album) Track \(number)"
    }
}
