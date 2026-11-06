import AppKit
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
    /// loop goes idle, so a tree read the same across an idle is done. Work
    /// off the main thread is waited for by its own event before this. A
    /// tree that never converges fails the wait rather than being inspected
    /// mid-layout.
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

    /// [`settle`] for a tree whose work runs off the main thread and tells
    /// no one when it ends: a lightbox decoding its image and reading its
    /// text, or a fixed query answering the reads of the storage manager.
    /// Its tree is read until no frame has moved for `Wait.steadiness`.
    @MainActor
    static func settleUnmoved(
        _ host: NSView,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        try await Wait.untilUnmoved(file: file, line: line) {
            frames(host)
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
