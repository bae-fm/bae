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
        viewport.setVisibleHeight(700)
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
        viewport.setVisibleHeight(700)
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
        viewport.setVisibleHeight(700)
        viewport.place(
            .detail("a1"),
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
        viewport.setVisibleHeight(700)
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
}
