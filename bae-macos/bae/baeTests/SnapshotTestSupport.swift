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
        let window = NSWindow(
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

}
