import AppKit
import SwiftUI

/// Shared AppKit hosting helpers for the view tests: a view hosted in a
/// window, settled, and its AppKit view tree.
enum SnapshotTestSupport {
    /// Let SwiftUI publish its renders before a hosted-view test inspects or
    /// interacts with it: lay the tree out until no frame in it has moved
    /// for `Wait.steadiness`.
    ///
    /// SwiftUI lays a hosted tree out over several main-actor turns — a
    /// geometry reader publishes a width, the views under it re-measure, a
    /// text wraps — and work a view starts on a task lands a few turns later
    /// and moves a row by a line. How many turns that takes depends on the
    /// machine, so convergence is what is waited for. A tree that never
    /// converges fails the wait rather than being inspected mid-layout.
    @MainActor
    static func settle(
        _ host: NSView,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        try await Wait.untilSteady(file: file, line: line) {
            autoreleasepool {
                host.layoutSubtreeIfNeeded()
                return descendants(of: host).map(\.frame)
            }
        }
    }

    /// Every AppKit view below `view`, depth first. SwiftUI controls may be
    /// nested below private hosting containers, so interaction tests use the
    /// full hosted tree rather than assuming one framework-specific depth.
    @MainActor
    static func descendants(of view: NSView) -> [NSView] {
        view.subviews.flatMap { [$0] + descendants(of: $0) }
    }
}
