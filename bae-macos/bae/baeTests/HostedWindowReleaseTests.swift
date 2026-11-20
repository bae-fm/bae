import AppKit
import SwiftUI
import Testing

@testable import bae

/// A hosted window is let go when its test ends, whoever handles the window
/// server's notices and pointer tracking about it. One that comes while the
/// test waits is handled by the test host's run loop, which XCTest runs from
/// inside a pool that drains only when the whole run ends.
@MainActor
@Suite("Hosted window release")
struct HostedWindowReleaseTests {
    @Test(
        "a window whose move notice the run loop handles is let go after its test"
    )
    func aWindowMovedDuringItsTestIsLetGo() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear,
            size: NSSize(width: 200, height: 100)
        ) { window, host in
            let hosted = try #require(window as? SnapshotTestWindow)
            let moves = hosted.moves
            hosted.setFrameOrigin(
                NSPoint(x: hosted.frame.minX + 10, y: hosted.frame.minY)
            )
            // Read at each idle, handling no notice itself, so the run loop
            // handles the window server's notice of the move.
            try await Wait.untilLaidOut(host) { hosted.moves > moves }
        }
    }

    @Test(
        "a window the pointer's tracking enters and leaves is let go after its test"
    )
    func aWindowThePointerCrossesIsLetGo() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear,
            size: NSSize(width: 200, height: 100)
        ) { window, host in
            let hosted = try #require(window as? SnapshotTestWindow)
            let crossings = hosted.crossings
            // When the displays change, the window server can move a hosted
            // window onto one, past where the pointer rests, and the
            // pointer's tracking then reports entering and leaving it.
            for type in [NSEvent.EventType.mouseEntered, .mouseExited] {
                let event = try #require(
                    NSEvent.enterExitEvent(
                        with: type,
                        location: .zero,
                        modifierFlags: [],
                        timestamp: 0,
                        windowNumber: window.windowNumber,
                        context: nil,
                        eventNumber: 0,
                        trackingNumber: 0,
                        userData: nil
                    )
                )
                NSApp.postEvent(event, atStart: false)
            }
            // Read at each idle, taking no event itself, so the run loop
            // takes both off the queue.
            try await Wait.untilLaidOut(host) {
                hosted.crossings >= crossings + 2
            }
        }
    }
}
