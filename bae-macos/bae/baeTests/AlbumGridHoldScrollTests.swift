import SwiftUI
import Testing

@testable import bae

@MainActor
@Suite("AlbumGridViewport hold scrolls")
struct AlbumGridHoldScrollTests {
    private let detail = AlbumGridSlot.detail(albumId: "a1")

    /// A viewport scrolled 6000 points down, showing 700.
    private func scrolled() -> AlbumGridViewport {
        let viewport = AlbumGridViewport()
        viewport.setScroll(
            AlbumGridScroll(
                offset: 6000,
                visibleHeight: 700,
                offsets: 0...12000
            )
        )
        return viewport
    }

    /// Lays out the open album's 500-point detail at `columnCount`, `minY`
    /// points below the top of the visible area.
    private func place(
        _ minY: CGFloat,
        in viewport: AlbumGridViewport,
        columnCount: Int = 3
    ) {
        viewport.place(
            .detail,
            as: detail,
            columnCount: columnCount,
            placed: AlbumGridPlacedCell(
                frame: CGRect(x: 0, y: minY, width: 200, height: 500),
                contentTop: 6000 + minY
            )
        )
    }

    /// Holds the detail as a change of column count holds the slot on top.
    private func holdForColumnChange(
        at minY: CGFloat,
        in viewport: AlbumGridViewport
    ) {
        place(minY, in: viewport)
        #expect(viewport.anchor(from: 3, to: 3) == detail)
    }

    private func held(
        _ reason: AlbumGridHold.Reason,
        contentTop: CGFloat
    ) -> AlbumGridHeldSlot {
        AlbumGridHeldSlot(
            hold: AlbumGridHold(slot: detail, reason: reason),
            contentTop: contentTop
        )
    }

    @Test("a slot a column change holds is scrolled back once per standing")
    func columnChangeScrollsBackOncePerStanding() {
        let viewport = scrolled()
        holdForColumnChange(at: 120, in: viewport)
        #expect(
            viewport.scrollBackingHeldSlot(columnCount: 3)
                == held(.columnChange, contentTop: 6120)
        )

        // Several reports in the frame the scroll was asked for, before it
        // takes effect, find the detail where it was: no second scroll.
        for _ in 0..<5 {
            place(120, in: viewport)
            viewport.setScroll(
                AlbumGridScroll(
                    offset: 6000,
                    visibleHeight: 700,
                    offsets: 0...12000
                )
            )
            #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)
        }

        // The detail moved: it is scrolled back again.
        place(90, in: viewport)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) != nil)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)

        // The content grew under the scroll with the detail where it was.
        viewport.setScroll(
            AlbumGridScroll(
                offset: 6000,
                visibleHeight: 700,
                offsets: 0...12400
            )
        )
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) != nil)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)

        // On top, it needs none.
        place(0, in: viewport)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)
    }

    @Test(
        "a slot a column change holds that reached the top and drifts back is scrolled"
    )
    func columnChangeScrollsAgainAfterDriftingBack() {
        let viewport = scrolled()
        holdForColumnChange(at: 120, in: viewport)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) != nil)

        // The scroll worked: the slot is on top.
        place(0, in: viewport)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)

        // The lazy grid then moves it back to where it stood before the
        // scroll, with the scroll where it stood: nothing is on its way, so
        // it needs the scroll again.
        place(120, in: viewport)
        #expect(
            viewport.scrollBackingHeldSlot(columnCount: 3)
                == held(.columnChange, contentTop: 6120)
        )
    }

    @Test("a revealed card is scrolled to once for each top it takes")
    func revealScrollsOncePerContentTop() {
        let viewport = scrolled()
        viewport.holdForReveal(detail)
        place(120, in: viewport)
        #expect(
            viewport.scrollBackingHeldSlot(columnCount: 3)
                == held(.reveal, contentTop: 6120)
        )

        // The scroll to that point stays in force: the content growing under
        // it asks for no second scroll.
        viewport.setScroll(
            AlbumGridScroll(
                offset: 6000,
                visibleHeight: 700,
                offsets: 0...12400
            )
        )
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)

        // Nor does the card reaching the top, or a layout that draws it back
        // where it was, at the same top in the content.
        viewport.place(
            .detail,
            as: detail,
            columnCount: 3,
            placed: AlbumGridPlacedCell(
                frame: CGRect(x: 0, y: 0, width: 200, height: 500),
                contentTop: 6120
            )
        )
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)
        place(120, in: viewport)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)

        // The card moved in the content: it is scrolled to again.
        place(90, in: viewport)
        #expect(
            viewport.scrollBackingHeldSlot(columnCount: 3)
                == held(.reveal, contentTop: 6090)
        )
    }

    @Test("a column change takes over a reveal's hold of the same slot")
    func columnChangeTakesOverRevealHold() {
        let viewport = scrolled()
        viewport.holdForReveal(detail)
        #expect(viewport.anchor(from: 3, to: 2) == detail)
        place(120, in: viewport, columnCount: 2)
        #expect(
            viewport.scrollBackingHeldSlot(columnCount: 2)
                == held(.columnChange, contentTop: 6120)
        )
    }
}
