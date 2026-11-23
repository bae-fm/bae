import AppKit
import Testing

@testable import bae

/// A watch of a window's notices, which the setup sheet and the traffic light
/// buttons keep for as long as their view lives.
@MainActor
@Suite("WindowNoticeWatch")
struct WindowNoticeWatchTests {
    private final class Count {
        var resizes = 0
    }

    private func window() -> NSWindow {
        NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 300, height: 200),
            styleMask: [.titled],
            backing: .buffered,
            defer: true
        )
    }

    @Test("a watch runs for each resize of its window")
    func runsOnResize() {
        let center = NotificationCenter.default
        let window = window()
        let count = Count()
        let watch = WindowNoticeWatch(of: window) { _ in
            count.resizes += 1
        }
        center.post(name: NSWindow.didResizeNotification, object: window)
        center.post(name: NSWindow.didResizeNotification, object: window)
        #expect(count.resizes == 2)
        withExtendedLifetime(watch) {}
    }

    @Test("a watch runs for the notices it names, and no resize")
    func runsOnTheNoticesItNames() {
        let center = NotificationCenter.default
        let window = window()
        let count = Count()
        let watch = WindowNoticeWatch(
            of: window,
            notices: [NSWindow.didBecomeKeyNotification]
        ) { _ in
            count.resizes += 1
        }
        center.post(name: NSWindow.didBecomeKeyNotification, object: window)
        center.post(name: NSWindow.didResizeNotification, object: window)
        #expect(count.resizes == 1)
        withExtendedLifetime(watch) {}
    }

    @Test("a freed watch is gone from its window, with no view to move away")
    func freedWatchRunsNoMore() {
        let center = NotificationCenter.default
        let window = window()
        let count = Count()
        // The watch goes out of scope as a closing sheet's views do.
        autoreleasepool {
            _ = WindowNoticeWatch(of: window) { _ in
                count.resizes += 1
            }
        }
        center.post(name: NSWindow.didResizeNotification, object: window)
        #expect(count.resizes == 0)
    }
}
