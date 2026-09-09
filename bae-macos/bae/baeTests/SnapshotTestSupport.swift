import AppKit
import SwiftUI
import Testing

/// Shared AppKit hosting + snapshot helpers for the view tests.
enum SnapshotTestSupport {
    /// Host `view` (sized to `size`) in a borderless key window. The caller keeps
    /// the returned window alive for the test's duration and uses the host to
    /// capture pixels or send events through the window.
    @MainActor
    static func hostInWindow<V: View>(
        _ view: V,
        size: NSSize
    ) -> (window: NSWindow, host: NSHostingView<V>) {
        let bounds = NSRect(origin: .zero, size: size)
        let host = NSHostingView(rootView: view)
        host.frame = bounds
        let window = SnapshotTestWindow(
            contentRect: bounds,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.contentView = host
        window.makeKeyAndOrderFront(nil)
        return (window, host)
    }

    /// Lay out `host` and capture it as PNG bytes. Yields once so SwiftUI's
    /// async work settles, and sleeps `waitNanoseconds` first when the view has
    /// async content (a cover load) that must resolve before the capture.
    @MainActor
    static func capturePNG(
        _ host: NSView,
        size: NSSize,
        waitNanoseconds: UInt64 = 0
    ) async throws -> Data {
        let bounds = NSRect(origin: .zero, size: size)
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        if waitNanoseconds > 0 {
            try await Task.sleep(nanoseconds: waitNanoseconds)
        }
        host.layoutSubtreeIfNeeded()
        let bitmap = try #require(
            host.bitmapImageRepForCachingDisplay(in: bounds)
        )
        host.cacheDisplay(in: bounds, to: bitmap)
        return try #require(bitmap.representation(using: .png, properties: [:]))
    }

    /// Let SwiftUI publish its renders before a hosted-view test inspects or
    /// interacts with it: yield until a turn changes no frame in the hosted
    /// tree.
    ///
    /// SwiftUI lays a hosted tree out over several main-actor turns — a
    /// geometry reader publishes a width, the views under it re-measure, a
    /// text wraps — and how many turns that takes depends on the machine. A
    /// fixed number of yields measured a tree mid-layout on a slow runner and
    /// gave frames a few points off. Convergence is what "settled" means, so
    /// that is what is waited for, after the floor of turns every hosted
    /// view needs to publish at all. A view that animates never converges;
    /// `maxTurns` bounds the wait and leaves it as the last turn drew it.
    @MainActor
    static func settle(
        _ host: NSView,
        minimumTurns: Int = 3,
        maxTurns: Int = 120
    ) async {
        var previous: [CGRect]?
        for turn in 0..<maxTurns {
            host.layoutSubtreeIfNeeded()
            let frames = descendants(of: host).map(\.frame)
            if turn >= minimumTurns, frames == previous {
                return
            }
            previous = frames
            await Task.yield()
        }
        host.layoutSubtreeIfNeeded()
    }

    /// Every AppKit view below `view`, depth first. SwiftUI controls may be
    /// nested below private hosting containers, so interaction tests use the
    /// full hosted tree rather than assuming one framework-specific depth.
    @MainActor
    static func descendants(of view: NSView) -> [NSView] {
        view.subviews.flatMap { [$0] + descendants(of: $0) }
    }

}

extension Collection<String> {
    /// Whether any of these lines carries `text`.
    ///
    /// Text recognition reads a line as it was drawn, glyphs included: a
    /// section header's symbol, an error's warning triangle, a checkbox's box
    /// — each shares a baseline with its words and comes back glued to them,
    /// and how much of the glyph survives depends on the machine that drew
    /// the pixels. A check that the words were drawn asks whether a line
    /// carries them, not whether one equals them.
    func carrying(_ text: String) -> Bool {
        contains { $0.contains(text) }
    }
}

private final class SnapshotTestWindow: NSWindow {
    override var canBecomeKey: Bool { true }
}
