import Foundation
import Observation

/// Collapse state for a screen header that yields its vertical room to the
/// scrolling content beneath it.
///
/// `progress` (0 = fully expanded, 1 = fully compact) is a pure function of
/// the scroll position of whichever pane the user drove last: within the
/// first `trackDistance` points it tracks the offset 1:1, so the header
/// scrubs closed with the first points of scroll and reopens only as the
/// content returns to the very top; past that distance it is simply compact.
/// One refinement on top of the position formula: a scroll that settles
/// mid-scrub snaps to the nearest end state rather than resting
/// half-collapsed.
///
/// Multiple panes may report (a split view's list and detail); the model
/// follows the most recent reporter, so the header always reflects the pane
/// the user is actually scrolling.
@Observable
@MainActor
final class HeaderCollapse {
    /// 0 = fully expanded, 1 = fully compact.
    private(set) var progress: Double = 0

    /// Scroll distance over which the header scrubs between its end states
    /// at the top of the content.
    static let trackDistance: Double = 64

    private var activeScroller: String?

    /// Report a pane's scroll position. `offset` is the content offset from
    /// the top edge (0 = at rest at the top), normalized for content insets
    /// and clamped to the scrollable span by the reporter.
    func reportScroll(scroller: String, offset: Double) {
        activeScroller = scroller
        progress = min(max(offset / Self.trackDistance, 0), 1)
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
