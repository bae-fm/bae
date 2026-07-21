import Testing

@testable import bae

/// Kinematics of the collapsing library header: 1:1 scrubbing at the top of
/// the content, compact everywhere below it, idle snapping, and per-scroller
/// hand-off.
@MainActor
@Suite("HeaderCollapse")
struct HeaderCollapseTests {
    /// Feed a sequence of offsets from one scroller, in order.
    private func feed(
        _ model: HeaderCollapse,
        scroller: String = "grid",
        offsets: [Double]
    ) {
        for offset in offsets {
            model.reportScroll(scroller: scroller, offset: offset)
        }
    }

    @Test("at the top, progress tracks the offset 1:1")
    func tracksOffsetInZone() {
        let model = HeaderCollapse()
        feed(model, offsets: [0])
        #expect(model.progress == 0)
        feed(model, offsets: [HeaderCollapse.trackDistance / 2])
        #expect(model.progress == 0.5)
        feed(model, offsets: [HeaderCollapse.trackDistance])
        #expect(model.progress == 1)
        feed(model, offsets: [HeaderCollapse.trackDistance / 2])
        #expect(model.progress == 0.5)
    }

    @Test("deep in the content, scrolling down stays compact")
    func staysCompactScrollingDownDeep() {
        let model = HeaderCollapse()
        feed(model, offsets: [0, 100, 400, 800])
        #expect(model.progress == 1)
    }

    @Test("scrolling up stays compact until the content nears the top")
    func staysCompactScrollingUpUntilTop() {
        let model = HeaderCollapse()
        feed(model, offsets: [0, 800])
        // A long upward scroll that stops short of the tracking zone must
        // not expand the header.
        feed(model, offsets: [600, 400, 200, HeaderCollapse.trackDistance])
        #expect(model.progress == 1)
        // Only crossing into the zone starts reopening it, 1:1 to the top.
        feed(model, offsets: [HeaderCollapse.trackDistance / 2])
        #expect(model.progress == 0.5)
        feed(model, offsets: [0])
        #expect(model.progress == 0)
    }

    @Test("settling mid-scrub snaps to the nearest end state")
    func idleSnapsToNearestState() {
        let model = HeaderCollapse()
        feed(model, offsets: [0, HeaderCollapse.trackDistance * 0.25])
        model.reportPhase(scroller: "grid", isScrolling: false)
        #expect(model.progress == 0)

        feed(model, offsets: [HeaderCollapse.trackDistance * 0.75])
        model.reportPhase(scroller: "grid", isScrolling: false)
        #expect(model.progress == 1)
    }

    @Test("a non-active scroller's idle does not snap")
    func inactiveScrollerIdleIgnored() {
        let model = HeaderCollapse()
        feed(model, offsets: [0, HeaderCollapse.trackDistance * 0.25])
        model.reportPhase(scroller: "other", isScrolling: false)
        #expect(model.progress == 0.25)
    }

    @Test("handing off to a pane resting at its top expands")
    func handOffAtTopExpands() {
        let model = HeaderCollapse()
        feed(model, scroller: "a", offsets: [0, 800])
        #expect(model.progress == 1)
        feed(model, scroller: "b", offsets: [0])
        #expect(model.progress == 0)
    }
}
