import AppKit
import SwiftUI
import XCTest

@testable import bae

/// The popovers the app shows, as each announces itself about to show.
@MainActor
private final class ShownPopovers {
    var popovers: [NSPopover] = []
}

final class PopoverAnimationTests: XCTestCase {
    /// A SwiftUI popover shows, and later resizes, without animating: the app
    /// turns the animation off for every popover it shows.
    @MainActor
    func testASwiftUIPopoverDoesNotAnimate() async throws {
        let shown = ShownPopovers()
        // A popover is placed on a display whatever its anchor's window is,
        // so the one this test opens is shown transparent to the eye and to
        // the pointer; the test reads the popover, not its pixels.
        let observer = NotificationCenter.default.addObserver(
            forName: NSPopover.willShowNotification,
            object: nil,
            queue: nil
        ) { notification in
            guard let popover = notification.object as? NSPopover else {
                preconditionFailure("will-show carried no popover")
            }
            MainActor.assumeIsolated {
                let window = popover.contentViewController?.view.window
                window?.alphaValue = 0
                window?.ignoresMouseEvents = true
                shown.popovers.append(popover)
            }
        }
        defer { NotificationCenter.default.removeObserver(observer) }
        let size = NSSize(width: 80, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear
                .frame(width: size.width, height: size.height)
                .popover(isPresented: .constant(true)) {
                    Color.clear.frame(width: 120, height: 80)
                },
            size: size
        ) { _, _ in
            try await Wait.until { shown.popovers.count == 1 }
            let popover = shown.popovers[0]
            XCTAssertFalse(popover.animates)
            popover.performClose(nil)
        }
    }
}
