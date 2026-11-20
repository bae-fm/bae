import AppKit
import SwiftUI
import Testing

@testable import bae

/// A hosted window is let go when its test ends, whoever handles the window
/// server's notices about it. A notice that comes while the test waits is
/// handled by the test host's run loop, which XCTest runs from inside a pool
/// that drains only when the whole run ends.
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
}
