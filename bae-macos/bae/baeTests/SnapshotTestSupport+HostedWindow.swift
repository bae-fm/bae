import AppKit
import SwiftUI
import Testing

extension SnapshotTestSupport {
    /// Host `view` (sized to `size`) in a borderless window past the edge of
    /// every display for the length of `body`, which uses the host to read
    /// its view tree or send events through the window. When `body`
    /// returns or throws, the window is closed and it and the view tree it
    /// held are let go; a window a press was sent into is hidden and kept
    /// with its tree for the rest of the run.
    ///
    /// The window never becomes key. Whether a window is key follows whether
    /// the test host is the active app, which is whatever the person at the
    /// machine last clicked; a window that is never key lays out the same
    /// controls every time. First responders and sent events do not need a
    /// key window.
    @MainActor
    static func withHostedWindow<V: View, Value>(
        _ view: V,
        size: NSSize,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ body: (NSWindow, NSHostingView<V>) async throws -> Value
    ) async throws -> Value {
        NoticePools.install()
        weak var released: NSWindow?
        var retired = false
        let result: Value
        do {
            let (window, host) = autoreleasepool {
                openWindow(hosting: view, size: size)
            }
            released = window
            Wait.announceRelease(of: window)
            result = try await body(window, host)
            // A click on a view with a double-tap gesture leaves a timer
            // that settles the gesture once a second click can no longer
            // come, and it reaches the view without owning it: a window and
            // tree let go before it fires take the process down with it.
            // Such a window is hidden and kept, tree included.
            if HostedInput.wasPressed(window) {
                retired = true
                autoreleasepool { hide(window) }
                HostedInput.keep(window)
            }
            else {
                autoreleasepool { close(window) }
            }
        }
        // Closed is not gone: a window something still holds lives on in
        // the process. XCTest drains an autorelease pool after each test,
        // which lets go of whatever that test's own AppKit calls kept;
        // Swift Testing drains none until the run ends, so under it a window
        // not let go here would stay for the rest of the run.
        if Test.current != nil, !retired {
            try await untilWindows(file: file, line: line) { released == nil }
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
        return (window, host)
    }

    /// Return once `condition` holds of AppKit's windows, which send no
    /// change notice: it is read at once and at each idle of the run loop,
    /// in a pool of its own. The wait ends on the windows' own state, not on
    /// a stretch with no notices.
    @MainActor
    static func untilWindows(
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: @MainActor () -> Bool
    ) async throws {
        let end = ContinuousClock.now.advanced(by: HangGuard.duration)
        while true {
            if autoreleasepool(invoking: condition) { return }
            try await Wait.nextIdle(
                before: end,
                file: file,
                line: line
            )
        }
    }

    /// Return once `window` has a sheet attached, or none, as `attached`
    /// says. A sheet begun or ended is not done when the call returns: the
    /// window server finishes it, and tells the app with notices.
    @MainActor
    static func untilSheet(
        on window: NSWindow,
        attached: Bool,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        try await untilWindows(file: file, line: line) {
            (window.attachedSheet != nil) == attached
        }
    }

    /// Take any sheet the test left on `window`, and then the window, off the
    /// screen.
    @MainActor
    private static func hide(_ window: NSWindow) {
        while let sheet = window.attachedSheet {
            window.endSheet(sheet)
            sheet.orderOut(nil)
        }
        window.orderOut(nil)
    }

    /// End a hosted window: any sheet the test left on it, the window, and
    /// the view tree it held.
    @MainActor
    private static func close(_ window: NSWindow) {
        hide(window)
        window.contentView = nil
        window.close()
    }

    /// A window origin past the right edge of every display, so a hosted
    /// view is never on the screen of the person running the suite: nothing
    /// flashes while it runs, and their pointer never hovers a hosted row.
    @MainActor
    private static var offscreenOrigin: NSPoint {
        let right = NSScreen.screens.map(\.frame.maxX).max() ?? 0
        return NSPoint(x: right + 1_000, y: 0)
    }
}

final class SnapshotTestWindow: NSWindow {
    /// A sheet — a confirmation dialog, an alert — brings the window it is
    /// on onto a display, sliding it in from past the edge, and shows
    /// itself there. Both stay transparent to the eye and to the pointer:
    /// the test presses the sheet's buttons itself.
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

    /// How many times the window server has said where it placed or moved
    /// this window.
    private(set) var moves = 0

    override func sendEvent(_ event: NSEvent) {
        if event.type == .appKitDefined, event.subtype == .windowMoved {
            moves += 1
        }
        super.sendEvent(event)
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

/// Hands each notice the window server sends the app to AppKit inside an
/// autorelease pool of its own, whoever takes it off the queue.
///
/// Handling a notice autoreleases the window it names. A notice that comes
/// while a Swift Testing test waits is handled by XCTest's waiter, which runs
/// the test host's run loop from inside a pool that drains only when the
/// whole run ends, so the window it named would outlive its test. Installed
/// once, for the rest of the run: AppKit calls the monitor from inside its
/// handling of each notice, and the monitor hands the notice back to AppKit
/// in a pool and stops AppKit's own handling.
@MainActor
private enum NoticePools {
    private static var monitor: Any?
    /// Whether the monitor is handing a notice back to AppKit, which calls
    /// the monitor again for it.
    private static var handing = false

    static func install() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .appKitDefined) {
            notice in
            let handed = MainActor.assumeIsolated { hand(notice) }
            return handed ? nil : notice
        }
    }

    /// Hand `notice` to AppKit in a pool of its own; false when the notice
    /// is the one being handed now, which AppKit then handles as it is.
    private static func hand(_ notice: NSEvent) -> Bool {
        if handing { return false }
        handing = true
        defer { handing = false }
        autoreleasepool { NSApp.sendEvent(notice) }
        return true
    }
}
