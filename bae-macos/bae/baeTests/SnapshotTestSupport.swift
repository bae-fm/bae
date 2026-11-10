import AppKit
import BaeKit
import SwiftUI

/// Shared AppKit hosting helpers for the view tests: a view hosted in a
/// window, settled, and its AppKit view tree.
enum SnapshotTestSupport {
    /// Let SwiftUI publish its renders before a hosted-view test inspects or
    /// interacts with it: lay the tree out at each idle of the main run loop
    /// until no frame in it moved across one.
    ///
    /// SwiftUI lays a hosted tree out over several main-actor turns — a
    /// geometry reader publishes a width, the views under it re-measure, a
    /// text wraps — and work a view starts on a main-actor task lands a few
    /// turns later and moves a row by a line. Each turn ends when the run
    /// loop goes idle, so a tree read the same across an idle is done, with
    /// one limit: an idle can come while main-actor work is still queued, so
    /// a chain of main-actor steps that moves no frame — a view's task that
    /// opens a read, which asks a step later — can read the same across an
    /// idle before it has done anything. A test whose tree waits on such a
    /// chain, or on work off the main thread, first waits for what it knows
    /// of it: a store's value, a fixture's unanswered reads, a load's phase.
    /// A tree that never converges fails the wait rather than being
    /// inspected mid-layout.
    @MainActor
    static func settle(
        _ host: NSView,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        try await Wait.untilSteady(file: file, line: line) {
            frames(host)
        }
    }

    /// [`settle`] for a tree reading fixed queries, which answer off the
    /// main thread: wait until `answers` has answered every read, then for
    /// the tree to come to rest, and again if resting asked for more. None
    /// unanswered also holds before anything is asked, so a tree whose
    /// first read is asked from a view's task waits for its answer first.
    @MainActor
    static func settle(
        _ host: NSView,
        answering answers: UnansweredCount,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        while true {
            try await Wait.until(file: file, line: line) {
                answers.value == 0
            }
            try await settle(host, file: file, line: line)
            if answers.value == 0 { return }
        }
    }

    @MainActor
    private static func frames(_ host: NSView) -> [CGRect] {
        autoreleasepool {
            host.layoutSubtreeIfNeeded()
            return descendants(of: host).map(\.frame)
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
