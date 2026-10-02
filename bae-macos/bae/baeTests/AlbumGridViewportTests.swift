import SwiftUI
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
        viewport.setScroll(showing700)
        for row in 0..<rows {
            for column in 0..<columnCount {
                let position = row * columnCount + column
                viewport.place(
                    .position(position),
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
            .position(9),
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
            .position(3),
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
        viewport.remove(.position(0), columnCount: 2)
        #expect(viewport.anchor(from: 2, to: 1) == .position(1))
    }

    @Test("the open album's detail can be the slot on top")
    func detailOnTop() {
        let viewport = AlbumGridViewport()
        viewport.setScroll(showing700)
        viewport.place(
            .detail,
            as: .detail(albumId: "a1"),
            columnCount: 2,
            frame: CGRect(x: 0, y: -100, width: 200, height: 500)
        )
        viewport.place(
            .position(2),
            as: .position(2),
            columnCount: 2,
            frame: CGRect(x: 0, y: 432, width: 200, height: 200)
        )
        #expect(viewport.anchor(from: 2, to: 3) == .detail(albumId: "a1"))
    }

    @Test("nothing in view keeps nothing on top")
    func nothingInView() {
        let viewport = AlbumGridViewport()
        viewport.setScroll(showing700)
        viewport.place(
            .position(0),
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

    @Test("a slot is shown while it is laid out in the visible area")
    func slotShown() {
        let viewport = viewport(rows: 4, columnCount: 2, top: 100)
        #expect(viewport.shows(.position(0)))
        #expect(viewport.shows(.position(4)))
        // Its row starts 790 points down, past the 700 in view.
        #expect(!viewport.shows(.position(6)))
        // Never laid out.
        #expect(!viewport.shows(.position(8)))
        // Scrolled past.
        let scrolled = self.viewport(rows: 4, columnCount: 2, top: -250)
        #expect(!scrolled.shows(.position(0)))
        #expect(scrolled.shows(.position(2)))
    }
}

/// Where a reveal goes next from one layout of its parts.
@MainActor
@Suite("RevealLayout")
struct RevealLayoutTests {
    @Test("a row in full view needs no scroll")
    func rowInViewStays() {
        #expect(
            step(row: CGRect(x: 0, y: 1000, width: 300, height: 40)) == .shown
        )
        #expect(
            step(row: CGRect(x: 0, y: 1660, width: 300, height: 40)) == .shown
        )
    }

    @Test("a row below or above the visible area is scrolled to its middle")
    func rowOutOfViewCentred() {
        // Partly under the bottom edge: its middle, 1000 + 690, goes to 350.
        #expect(
            step(row: CGRect(x: 0, y: 1670, width: 300, height: 40))
                == .scroll(to: 1340)
        )
        // Wholly above the top.
        #expect(
            step(row: CGRect(x: 0, y: 500, width: 300, height: 40))
                == .scroll(to: 170)
        )
    }

    @Test("the scroll stays inside the content")
    func targetClamped() {
        #expect(
            step(row: CGRect(x: 0, y: 0, width: 300, height: 40))
                == .scroll(to: 0)
        )
        #expect(
            step(row: CGRect(x: 0, y: 4980, width: 300, height: 40))
                == .scroll(to: 4300)
        )
    }

    @Test("a view taller than the visible area goes to the top")
    func tallViewToTop() {
        #expect(
            step(row: CGRect(x: 0, y: 1200, width: 300, height: 900))
                == .scroll(to: 1200)
        )
    }

    @Test("where a part goes does not depend on where the scroll was measured")
    func targetIndependentOfOffset() {
        let row = CGRect(x: 16, y: 19818, width: 1148, height: 40)
        for offset: CGFloat in [0, 9000, 18392] {
            let layout = RevealLayout(
                seq: 2,
                row: row,
                scroll: AlbumGridScroll(
                    offset: offset,
                    visibleHeight: 720,
                    offsets: 0...36290
                )
            )
            #expect(
                layout.step(showingTrack: true, findsCard: true)
                    == .scroll(to: 19478)
            )
        }
    }

    @Test("a track's album goes to the top until the track's row is laid out")
    func trackAlbumToTopFirst() {
        let card = CGRect(x: 0, y: 3000, width: 200, height: 280)
        #expect(step(card: card, showingTrack: true) == .scroll(to: 3000))
        // There, the row is waited for.
        #expect(step(card: card, showingTrack: true, at: 3000) == nil)
        // The detail laid out without the row is no answer for a track.
        #expect(
            step(
                card: CGRect(x: 0, y: 1000, width: 200, height: 280),
                detail: CGRect(x: 0, y: 1310, width: 1100, height: 400),
                showingTrack: true
            ) == nil
        )
    }

    @Test("an album on top is shown once its detail is laid out")
    func albumShownWithItsDetail() {
        let card = CGRect(x: 0, y: 1000, width: 200, height: 280)
        #expect(step(card: card) == nil)
        #expect(
            step(
                card: card,
                detail: CGRect(x: 0, y: 1310, width: 1100, height: 400)
            ) == .shown
        )
    }

    @Test("an album is scrolled to the top, and shown once it is there")
    func albumScrolledToTop() {
        let card = CGRect(x: 0, y: 1400, width: 200, height: 280)
        let detail = CGRect(x: 0, y: 1710, width: 1100, height: 400)
        #expect(step(card: card) == .scroll(to: 1400))
        #expect(step(card: card, detail: detail) == .scroll(to: 1400))
        #expect(step(card: card, detail: detail, at: 1400) == .shown)
    }

    @Test(
        "an album the content cannot bring to the top goes as near as it goes"
    )
    func albumNearTheEnd() {
        let card = CGRect(x: 0, y: 4500, width: 200, height: 280)
        let detail = CGRect(x: 0, y: 4810, width: 1100, height: 180)
        #expect(step(card: card, detail: detail) == .scroll(to: 4300))
        #expect(step(card: card, detail: detail, at: 4300) == .shown)
    }

    @Test("nothing laid out yet decides nothing")
    func nothingLaidOut() {
        #expect(step(showingTrack: true, findsCard: false) == nil)
        #expect(step(findsCard: false) == nil)
    }

    @Test("a card not laid out is found once its position is known")
    func cardNotLaidOutIsFound() {
        #expect(step(showingTrack: true) == .findCard)
        #expect(step() == .findCard)
        // The detail laid out does not place the card.
        #expect(
            step(detail: CGRect(x: 0, y: 1310, width: 1100, height: 400))
                == .findCard
        )
    }

    /// The step from a layout scrolled `at` points down 5000 points of
    /// content, showing 700.
    private func step(
        card: CGRect? = nil,
        detail: CGRect? = nil,
        row: CGRect? = nil,
        showingTrack: Bool? = nil,
        findsCard: Bool = true,
        at offset: CGFloat = 1000
    ) -> RevealStep? {
        RevealLayout(
            seq: 2,
            card: card,
            detail: detail,
            row: row,
            scroll: AlbumGridScroll(
                offset: offset,
                visibleHeight: 700,
                offsets: 0...4300
            )
        )
        .step(showingTrack: showingTrack ?? (row != nil), findsCard: findsCard)
    }
}

/// A reveal's steps as layouts place its parts, and where a card not laid
/// out is looked for.
@MainActor
@Suite("AlbumGridViewport revealing")
struct AlbumGridViewportRevealTests {
    private let album = PendingAlbumReveal(albumId: "a9", trackId: nil, seq: 2)

    @Test("a layout counts for its own reveal only")
    func layoutOfAnotherReveal() {
        let viewport = AlbumGridViewport()
        viewport.placeReveal(layout(card: 1000, seq: 1))
        #expect(viewport.step(of: album, findsCard: true) == nil)
        viewport.placeReveal(layout(card: 1000))
        #expect(viewport.step(of: album, findsCard: true) == .shown)
    }

    @Test("a card is looked for counting rows from the slot on top")
    func cardFoundFromTheSlotOnTop() {
        let viewport = AlbumGridViewport()
        // 50 rows of 300 points, showing 700, scrolled to 3000.
        viewport.setScroll(
            AlbumGridScroll(
                offset: 3000,
                visibleHeight: 700,
                offsets: 0...14300
            )
        )
        // The slot on top is 20 points under the top, in row 10.
        viewport.place(
            .position(40),
            as: .position(40),
            columnCount: 4,
            frame: CGRect(x: 0, y: 20, width: 200, height: 280)
        )
        let rowOf: (AlbumGridSlot) -> Int? = { slot in
            guard case .position(let position) = slot else { return nil }
            return position / 4
        }
        #expect(viewport.offset(toRow: 30, of: 50, rowOf: rowOf) == 9020)
        // Past the end, as near as the content scrolls.
        #expect(viewport.offset(toRow: 49, of: 50, rowOf: rowOf) == 14300)
    }

    @Test("with nothing laid out, a card is looked for down the whole content")
    func cardFoundDownTheContent() {
        let viewport = AlbumGridViewport()
        viewport.setScroll(
            AlbumGridScroll(offset: 0, visibleHeight: 700, offsets: 0...14300)
        )
        #expect(
            viewport.offset(toRow: 30, of: 50, rowOf: { _ in nil }) == 9000
        )
    }

    @Test("a card is not looked for before the scroll view has reported")
    func noScrollNoEstimate() {
        #expect(
            AlbumGridViewport().offset(toRow: 3, of: 50, rowOf: { _ in 0 })
                == nil
        )
    }

    /// The album's 280-point card `card` points down 40000 points of
    /// content, its detail under it, measured with the card on top.
    private func layout(card: CGFloat, seq: Int = 2) -> RevealLayout {
        RevealLayout(
            seq: seq,
            card: CGRect(x: 0, y: card, width: 200, height: 280),
            detail: CGRect(x: 0, y: card + 310, width: 1100, height: 400),
            scroll: AlbumGridScroll(
                offset: card,
                visibleHeight: 700,
                offsets: 0...39300
            )
        )
    }
}

/// At the top of content no taller than the 700 points in view.
private let showing700 = AlbumGridScroll(
    offset: 0,
    visibleHeight: 700,
    offsets: 0...0
)
