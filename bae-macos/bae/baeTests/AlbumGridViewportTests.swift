import CoreGraphics
import Testing

@testable import bae

@MainActor
@Suite("AlbumGridViewport")
struct AlbumGridViewportTests {
    /// A 700-point visible area with rows of 200-point cards laid out at
    /// `columnCount`, the first row `top` points below the top.
    private func viewport(
        rows: Int,
        columnCount: Int,
        top: CGFloat
    ) -> AlbumGridViewport {
        let viewport = AlbumGridViewport()
        viewport.setScroll(AlbumGridScroll(visibleHeight: 700))
        for row in 0..<rows {
            for column in 0..<columnCount {
                let position = row * columnCount + column
                viewport.place(
                    .album("a\(position)"),
                    as: .position(position),
                    columnCount: columnCount,
                    frame: CGRect(
                        x: CGFloat(column) * 230,
                        y: top + CGFloat(row) * 230,
                        width: 200,
                        height: 200
                    )
                )
            }
        }
        return viewport
    }

    @Test("the slot on top is the leftmost of the top row in view")
    func topRowLeftmost() {
        let viewport = viewport(rows: 3, columnCount: 4, top: -20)
        #expect(viewport.anchor(from: 4, to: 3) == .position(0))
    }

    @Test("a row mostly scrolled past gives way to the one under it")
    func mostlyScrolledPastRowSkipped() {
        let viewport = viewport(rows: 3, columnCount: 4, top: -150)
        #expect(viewport.anchor(from: 4, to: 3) == .position(4))
    }

    @Test("frames laid out at another column count do not count")
    func otherColumnCountIgnored() {
        let viewport = viewport(rows: 3, columnCount: 4, top: 0)
        // The first layout at three columns reports before the change is seen.
        viewport.place(
            .album("a9"),
            as: .position(9),
            columnCount: 3,
            frame: CGRect(x: 0, y: -100, width: 200, height: 400)
        )
        #expect(viewport.anchor(from: 4, to: 3) == .position(0))
    }

    @Test("the slot kept on top holds until the person scrolls")
    func heldUntilReleased() {
        let viewport = viewport(rows: 3, columnCount: 4, top: 0)
        #expect(viewport.anchor(from: 4, to: 3) == .position(0))
        // At three columns another slot is on top, but going back keeps the
        // same one.
        viewport.place(
            .album("a3"),
            as: .position(3),
            columnCount: 3,
            frame: CGRect(x: 0, y: 0, width: 260, height: 260)
        )
        #expect(viewport.anchor(from: 3, to: 4) == .position(0))

        viewport.release()
        #expect(viewport.anchor(from: 4, to: 3) == nil)
    }

    @Test("a slot that left the grid is not on top")
    func removedSlotIgnored() {
        let viewport = viewport(rows: 2, columnCount: 2, top: 0)
        viewport.remove(.album("a0"), columnCount: 2)
        #expect(viewport.anchor(from: 2, to: 1) == .position(1))
    }

    @Test("a position's album and its placeholder are separate cells")
    func placeholderLeavingKeepsAlbum() {
        let viewport = AlbumGridViewport()
        viewport.setScroll(AlbumGridScroll(visibleHeight: 700))
        let frame = CGRect(x: 0, y: 0, width: 200, height: 200)
        viewport.place(
            .album("a0"),
            as: .position(0),
            columnCount: 2,
            frame: frame
        )
        // The placeholder the album replaced leaves after the album arrived.
        viewport.remove(.placeholder(0), columnCount: 2)
        #expect(viewport.anchor(from: 2, to: 1) == .position(0))
    }

    @Test("the open album's detail can be the slot on top")
    func detailOnTop() {
        let viewport = AlbumGridViewport()
        viewport.setScroll(AlbumGridScroll(visibleHeight: 700))
        viewport.place(
            .detail,
            as: .detail(albumId: "a1"),
            columnCount: 2,
            frame: CGRect(x: 0, y: -100, width: 200, height: 500)
        )
        viewport.place(
            .album("a2"),
            as: .position(2),
            columnCount: 2,
            frame: CGRect(x: 0, y: 432, width: 200, height: 200)
        )
        #expect(viewport.anchor(from: 2, to: 3) == .detail(albumId: "a1"))
    }

    @Test("nothing in view keeps nothing on top")
    func nothingInView() {
        let viewport = AlbumGridViewport()
        viewport.setScroll(AlbumGridScroll(visibleHeight: 700))
        viewport.place(
            .album("a0"),
            as: .position(0),
            columnCount: 2,
            frame: CGRect(x: 0, y: 800, width: 200, height: 200)
        )
        #expect(viewport.anchor(from: 2, to: 3) == nil)
    }

    @Test("a slot is on top when its top meets the top at that column count")
    func onTop() {
        let viewport = viewport(rows: 2, columnCount: 2, top: 0)
        #expect(viewport.isOnTop(.position(0), columnCount: 2))
        #expect(viewport.isOnTop(.position(1), columnCount: 2))
        #expect(!viewport.isOnTop(.position(2), columnCount: 2))
        #expect(!viewport.isOnTop(.position(0), columnCount: 3))
    }

    @Test("the held slot is the one the last change kept on top")
    func heldSlot() {
        let viewport = viewport(rows: 2, columnCount: 2, top: 0)
        #expect(viewport.held == nil)
        _ = viewport.anchor(from: 2, to: 3)
        #expect(viewport.held == .position(0))
        viewport.release()
        #expect(viewport.held == nil)
    }

    /// Scrolled 1000 points down 5000 points of content, showing 700.
    private func placedRow(_ frame: CGRect) -> PlacedTrackRow {
        PlacedTrackRow(
            trackId: "t1",
            seq: 2,
            frame: frame,
            scroll: AlbumGridScroll(
                offset: 1000,
                visibleHeight: 700,
                contentHeight: 5000
            )
        )
    }

    @Test("a row in full view needs no scroll")
    func rowInViewStays() {
        #expect(
            placedRow(CGRect(x: 0, y: 1000, width: 300, height: 40))
                .scrollOffset
                == nil
        )
        #expect(
            placedRow(CGRect(x: 0, y: 1660, width: 300, height: 40))
                .scrollOffset
                == nil
        )
    }

    @Test("a track target does not depend on when the scroll offset arrives")
    func rowTargetIndependentOfScrollDelivery() {
        let viewport = AlbumGridViewport()
        let frame = CGRect(x: 16, y: 19818, width: 1148, height: 40)
        let row = PlacedTrackRow(
            trackId: "t1",
            seq: 2,
            frame: frame,
            scroll: AlbumGridScroll(
                offset: 18392,
                visibleHeight: 720,
                contentHeight: 37010
            )
        )
        viewport.placeRevealedRow(row)
        for offset: CGFloat in [0, 18392] {
            viewport.setScroll(
                AlbumGridScroll(
                    offset: offset,
                    visibleHeight: 720,
                    contentHeight: 37010
                )
            )
            #expect(viewport.revealedRow(seq: 2)?.scrollOffset == 19478)
        }
    }

    @Test("a row below or above the visible area is scrolled to its middle")
    func rowOutOfViewCentred() {
        // Partly under the bottom edge: its middle, 1000 + 690, goes to 350.
        #expect(
            placedRow(CGRect(x: 0, y: 1670, width: 300, height: 40))
                .scrollOffset
                == 1340
        )
        // Wholly above the top.
        #expect(
            placedRow(
                CGRect(x: 0, y: 500, width: 300, height: 40)
            )
            .scrollOffset
                == 170
        )
    }

    @Test("the scroll stays inside the content")
    func targetClamped() {
        #expect(
            placedRow(
                CGRect(x: 0, y: 0, width: 300, height: 40)
            )
            .scrollOffset
                == 0
        )
        #expect(
            placedRow(
                CGRect(x: 0, y: 4980, width: 300, height: 40)
            )
            .scrollOffset
                == 4300
        )
    }

    @Test("a view taller than the visible area goes to the top")
    func tallViewToTop() {
        #expect(
            placedRow(
                CGRect(x: 0, y: 1200, width: 300, height: 900)
            )
            .scrollOffset
                == 1200
        )
    }

    @Test("a revealed row counts for its own reveal, and once at the album")
    func revealedRowBySeq() {
        let viewport = AlbumGridViewport()
        let row = placedRow(CGRect(x: 0, y: 1900, width: 300, height: 40))
        viewport.placeRevealedRow(row)
        #expect(viewport.revealedRow(seq: 2) == row)
        #expect(viewport.revealedRow(seq: 1) == nil)
        // Laid out on the way to the album, it waits for the scroll to stop.
        #expect(viewport.revealedRow(seq: 2, atAlbum: true) == nil)
        viewport.revealReachedAlbum(seq: 2)
        #expect(viewport.revealedRow(seq: 2, atAlbum: true) == row)

        viewport.placeRevealedRow(nil)
        #expect(viewport.revealedRow(seq: 2) == nil)
    }
}
