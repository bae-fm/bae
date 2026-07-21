import Foundation
import Observation

/// Collapse state for a screen header that yields its vertical room to the
/// scrolling content beneath it.
///
/// `progress` (0 = fully expanded, 1 = fully compact) is derived from the
/// scroll reports of whichever scrollable pane the user drove last, with the
/// standard collapsing-header kinematics:
///
/// - **Near the top** (`offset ≤ trackDistance`): progress tracks the offset
///   1:1, so the header scrubs closed with the first points of scroll and
///   scrubs open again as the content returns to the top. A pure function of
///   position — nothing to jitter.
/// - **Deep in the content**: direction with hysteresis. Scrolling down keeps
///   the header compact. Scrolling up re-expands it only after
///   `expandTravel` points of accumulated upward travel; scrolling down again
///   re-collapses it after `collapseTravel` points. Each direction reversal
///   resets the opposite accumulator, so small jitters never flip the state.
/// - **Expanded on the way up**: the header stays open through the tracking
///   zone; reaching the very top re-arms 1:1 tracking for the next scroll.
/// - **Scroll settles mid-scrub**: the header snaps to the nearest end state
///   rather than resting half-collapsed.
///
/// Multiple panes may report (a split view's list and detail); the model
/// follows the most recent reporter and resets its travel accumulators on
/// each hand-off, so momentum in one pane never bleeds into another.
@Observable
@MainActor
final class HeaderCollapse {
    /// 0 = fully expanded, 1 = fully compact.
    private(set) var progress: Double = 0

    /// Scroll distance over which the header scrubs between its end states
    /// near the top of the content.
    static let trackDistance: Double = 64
    /// Accumulated upward travel, deep in the content, that re-expands the
    /// header.
    static let expandTravel: Double = 120
    /// Accumulated downward travel that re-collapses a header expanded deep
    /// in the content.
    static let collapseTravel: Double = 60

    private enum Latch {
        case collapsed
        case expanded
    }

    /// The resting state deep in the content, where position no longer
    /// dictates progress. Meaningless inside the tracking zone except as the
    /// "arrived expanded from below" marker.
    private var latch: Latch = .collapsed
    private var activeScroller: String?
    private var lastOffset: Double = 0
    private var upTravel: Double = 0
    private var downTravel: Double = 0

    /// Report a pane's scroll position. `offset` is the content offset from
    /// the top edge (0 = at rest at the top), normalized for content insets.
    func reportScroll(scroller: String, offset: Double) {
        if scroller != activeScroller {
            activeScroller = scroller
            lastOffset = offset
            upTravel = 0
            downTravel = 0
        }
        let delta = offset - lastOffset
        lastOffset = offset

        if offset <= Self.trackDistance {
            upTravel = 0
            downTravel = 0
            if latch == .expanded {
                progress = 0
                if offset <= 0 {
                    latch = .collapsed
                }
            }
            else {
                progress = min(max(offset / Self.trackDistance, 0), 1)
            }
        }
        else {
            if delta > 0 {
                downTravel += delta
                upTravel = 0
                if latch == .expanded, downTravel >= Self.collapseTravel {
                    latch = .collapsed
                }
            }
            else if delta < 0 {
                upTravel -= delta
                downTravel = 0
                if latch == .collapsed, upTravel >= Self.expandTravel {
                    latch = .expanded
                }
            }
            progress = latch == .expanded ? 0 : 1
        }
    }

    /// Report a pane's scroll phase. When the active pane's scroll settles
    /// while the header is mid-scrub, the header resolves to the nearest end
    /// state; the next in-zone scroll event re-derives progress from the
    /// offset.
    func reportPhase(scroller: String, isScrolling: Bool) {
        guard !isScrolling, scroller == activeScroller else {
            return
        }
        if progress > 0, progress < 1 {
            progress = progress < 0.5 ? 0 : 1
        }
    }
}
