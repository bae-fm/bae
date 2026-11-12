import SwiftUI
import Testing

@testable import bae

@MainActor
@Suite("AlbumGridViewport hold scrolls")
struct AlbumGridHoldScrollTests {
    @Test("a detail off the top is scrolled back once per standing")
    func detailScrolledBackOncePerStanding() {
        let viewport = AlbumGridViewport()
        let detail = AlbumGridSlot.detail(albumId: "a1")
        let scroll = AlbumGridScroll(
            offset: 6000,
            visibleHeight: 700,
            offsets: 0...12000
        )
        viewport.setScroll(scroll)
        let place = { (minY: CGFloat) in
            viewport.place(
                .detail,
                as: detail,
                columnCount: 3,
                frame: CGRect(x: 0, y: minY, width: 200, height: 500)
            )
        }
        viewport.hold(detail)
        place(120)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == detail)

        // Several reports in the frame the scroll was asked for, before it
        // takes effect, find the detail where it was: no second scroll.
        for _ in 0..<5 {
            place(120)
            viewport.setScroll(scroll)
            #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)
        }

        // The detail moved: it is scrolled back again.
        place(90)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == detail)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)

        // The content grew under the scroll with the detail where it was.
        viewport.setScroll(
            AlbumGridScroll(
                offset: 6000,
                visibleHeight: 700,
                offsets: 0...12400
            )
        )
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == detail)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)

        // On top, it needs none; a new hold starts again.
        place(0)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)
        viewport.hold(detail)
        place(40)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == detail)
    }

    @Test(
        "a slot that reached the top and drifts back to where it was is scrolled"
    )
    func driftingBackToTheLastStandingScrolls() {
        let viewport = AlbumGridViewport()
        let detail = AlbumGridSlot.detail(albumId: "a1")
        viewport.setScroll(
            AlbumGridScroll(
                offset: 6000,
                visibleHeight: 700,
                offsets: 0...12000
            )
        )
        let place = { (minY: CGFloat) in
            viewport.place(
                .detail,
                as: detail,
                columnCount: 3,
                frame: CGRect(x: 0, y: minY, width: 200, height: 500)
            )
        }
        viewport.hold(detail)
        place(120)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == detail)

        // The scroll worked: the slot is on top.
        place(0)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == nil)

        // The lazy grid then moves it back to where it stood before the
        // scroll, with the scroll where it stood: nothing is on its way, so
        // it needs the scroll again.
        place(120)
        #expect(viewport.scrollBackingHeldSlot(columnCount: 3) == detail)
    }
}
