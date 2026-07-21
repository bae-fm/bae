import Testing

@testable import bae

/// Kinematics of the collapsing library header: 1:1 scrubbing near the top,
/// direction-with-hysteresis deep in the content, latch hand-over at the top,
/// idle snapping, and per-scroller accumulator isolation.
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

    @Test("near the top, progress tracks the offset 1:1")
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

    @Test("upward travel below the threshold does not expand")
    func smallUpwardTravelKeepsCompact() {
        let model = HeaderCollapse()
        feed(model, offsets: [0, 400])
        feed(model, offsets: [400 - HeaderCollapse.expandTravel + 1])
        #expect(model.progress == 1)
    }

    @Test("enough upward travel deep in the content expands")
    func upwardTravelExpands() {
        let model = HeaderCollapse()
        feed(model, offsets: [0, 400])
        feed(model, offsets: [400 - HeaderCollapse.expandTravel])
        #expect(model.progress == 0)
    }

    @Test("a direction reversal resets the upward accumulator")
    func reversalResetsUpwardTravel() {
        let model = HeaderCollapse()
        feed(model, offsets: [0, 800])
        // 60 up, 20 down, 60 up: 120 total upward, but never 120 consecutive.
        feed(model, offsets: [740, 760, 700])
        #expect(model.progress == 1)
    }

    @Test("expanded deep, downward travel below the threshold keeps it open")
    func smallDownwardTravelKeepsExpanded() {
        let model = HeaderCollapse()
        feed(model, offsets: [0, 800, 800 - HeaderCollapse.expandTravel])
        #expect(model.progress == 0)
        feed(
            model,
            offsets: [
                800 - HeaderCollapse.expandTravel
                    + HeaderCollapse.collapseTravel - 1
            ]
        )
        #expect(model.progress == 0)
    }

    @Test("expanded deep, enough downward travel re-collapses")
    func downwardTravelRecollapses() {
        let model = HeaderCollapse()
        feed(model, offsets: [0, 800, 800 - HeaderCollapse.expandTravel])
        #expect(model.progress == 0)
        feed(
            model,
            offsets: [
                800 - HeaderCollapse.expandTravel
                    + HeaderCollapse.collapseTravel
            ]
        )
        #expect(model.progress == 1)
    }

    @Test("expanded on the way up, the header stays open through the zone")
    func expandedRidesThroughZoneToTop() {
        let model = HeaderCollapse()
        feed(model, offsets: [0, 800, 800 - HeaderCollapse.expandTravel])
        #expect(model.progress == 0)
        // Crossing the tracking zone while expanded must not re-collapse.
        feed(model, offsets: [HeaderCollapse.trackDistance - 4])
        #expect(model.progress == 0)
        // Reaching the top re-arms 1:1 tracking for the next scroll down.
        feed(model, offsets: [0])
        #expect(model.progress == 0)
        feed(model, offsets: [HeaderCollapse.trackDistance / 2])
        #expect(model.progress == 0.5)
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
        #expect(model.progress == HeaderCollapse.trackDistance * 0.25 / HeaderCollapse.trackDistance)
    }

    @Test("a scroller hand-off resets the travel accumulators")
    func handOffResetsTravel() {
        let model = HeaderCollapse()
        // Scroller a accumulates upward travel just short of the threshold.
        feed(model, scroller: "a", offsets: [0, 800])
        feed(
            model,
            scroller: "a",
            offsets: [800 - HeaderCollapse.expandTravel + 1]
        )
        #expect(model.progress == 1)
        // Scroller b's first report is a baseline, and its small upward
        // travel must start from zero, not inherit a's.
        feed(model, scroller: "b", offsets: [500, 470])
        #expect(model.progress == 1)
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
