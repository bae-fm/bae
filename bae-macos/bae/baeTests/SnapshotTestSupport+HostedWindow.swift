import AppKit
import SwiftUI
import Testing

extension SnapshotTestSupport {
    /// Host `view` (sized to `size`) in a borderless window past the edge of
    /// every display for the length of `body`, which uses the host to
    /// capture pixels or send events through the window. When `body`
    /// returns or throws, the window is closed and it and the view tree it
    /// held are let go.
    ///
    /// The window never becomes key. Whether a window is key follows whether
    /// the test host is the active app, which is whatever the person at the
    /// machine last clicked: a prominent button drew its accent in one
    /// capture and grey in the next, and two captures a test compared pixel
    /// for pixel differed there. A window that is never key draws the same
    /// controls every time. First responders and sent events do not need a
    /// key window.
    ///
    /// The tree's layers are set to draw at `captureScale` as soon as they
    /// exist, so the redraw that a display at another scale needs happens
    /// while the caller settles the view, not between two captures a test
    /// compares pixel for pixel.
    @MainActor
    static func withHostedWindow<V: View, Value>(
        _ view: V,
        size: NSSize,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ body: (NSWindow, NSHostingView<V>) async throws -> Value
    ) async throws -> Value {
        weak var released: NSWindow?
        let result: Value
        do {
            let (window, host) = autoreleasepool {
                openWindow(hosting: view, size: size)
            }
            released = window
            result = try await body(window, host)
            // A click on a view with a double-tap gesture leaves a timer
            // that settles the gesture once a second click can no longer
            // come, and it reaches the view without owning it: a tree let go
            // before it fires takes the process down with it. The window
            // stays until that time has passed.
            if let settled = HostedInput.gesturesSettle(in: window) {
                try await Wait.until(file: file, line: line) {
                    ContinuousClock.now >= settled
                }
            }
            autoreleasepool { close(window) }
        }
        // Closed is not gone: a window something still holds lives on in
        // the process. XCTest drains an autorelease pool after each test,
        // which lets go of whatever that test's own AppKit calls kept;
        // Swift Testing drains none until the run ends, so under it a window
        // not let go here would stay for the rest of the run.
        if Test.current != nil {
            try await Wait.until(file: file, line: line) { released == nil }
        }
        return result
    }

    @MainActor
    private static func openWindow<V: View>(
        hosting view: V,
        size: NSSize
    ) -> (window: NSWindow, host: NSHostingView<V>) {
        let host = NSHostingView(rootView: view)
        host.frame = NSRect(origin: .zero, size: size)
        let window = SnapshotTestWindow(
            contentRect: NSRect(origin: offscreenOrigin, size: size),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        // The window is closed and then let go by `withHostedWindow`; one
        // that released itself on close would be released a second time.
        window.isReleasedWhenClosed = false
        // Ordering a window in plays its zoom-in on a thread of its own,
        // paced by the display the window is on. Past every display there
        // is none, so the animation never ends and its thread is never
        // given back: a full run held more than a hundred of them, and a
        // later dispatch waited for a thread that never came.
        window.animationBehavior = .none
        window.contentView = host
        window.orderFront(nil)
        host.layoutSubtreeIfNeeded()
        if let layer = host.layer {
            rescale(layer)
        }
        if Test.current != nil {
            takePlacementNotice(for: window)
        }
        return (window, host)
    }

    /// Wait for the notice the window server sends once it has placed a
    /// window ordered in, handling it and anything else it sends meanwhile
    /// in the caller's autorelease pool.
    ///
    /// The notice comes a tenth of a second after the window is ordered in,
    /// and handling it autoreleases the window. Left to the test host's run
    /// loop it is handled in the pool the test runner holds for the whole
    /// run, which Swift Testing never drains, and the window then outlived
    /// its test. XCTest drains a pool after each of its tests, so a window
    /// hosted there needs no wait.
    @MainActor
    private static func takePlacementNotice(for window: SnapshotTestWindow) {
        let deadline = Date(timeIntervalSinceNow: 2)
        while !window.isPlaced,
            let notice = NSApp.nextEvent(
                matching: .appKitDefined,
                until: deadline,
                inMode: .default,
                dequeue: true
            )
        {
            NSApp.sendEvent(notice)
        }
    }

    /// End a hosted window: any sheet the test left on it, the window, and
    /// the view tree it held.
    @MainActor
    private static func close(_ window: NSWindow) {
        while let sheet = window.attachedSheet {
            window.endSheet(sheet)
            sheet.orderOut(nil)
        }
        window.orderOut(nil)
        window.contentView = nil
        window.close()
    }

    /// A window origin past the right edge of every display, so a hosted
    /// view is never on the screen of the person running the suite: nothing
    /// flashes while it runs, and their pointer never hovers a captured row.
    @MainActor
    private static var offscreenOrigin: NSPoint {
        let right = NSScreen.screens.map(\.frame.maxX).max() ?? 0
        return NSPoint(x: right + 1_000, y: 0)
    }
}

private final class SnapshotTestWindow: NSWindow {
    /// A sheet — a confirmation dialog, an alert — brings the window it is
    /// on onto a display, sliding it in from past the edge, and shows
    /// itself there. Both stay transparent to the eye and to the pointer:
    /// the test presses the sheet's buttons itself, and a capture draws the
    /// views, not the window.
    override func beginSheet(
        _ sheetWindow: NSWindow,
        completionHandler handler: ((NSApplication.ModalResponse) -> Void)? =
            nil
    ) {
        for window in [self, sheetWindow] {
            window.alphaValue = 0
            window.ignoresMouseEvents = true
        }
        super.beginSheet(sheetWindow, completionHandler: handler)
    }

    /// Whether the window server has said where it placed this window.
    private(set) var isPlaced = false

    override func sendEvent(_ event: NSEvent) {
        if event.type == .appKitDefined, event.subtype == .windowMoved {
            isPlaced = true
        }
        super.sendEvent(event)
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}
