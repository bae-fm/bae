import AppKit
import SwiftUI
import Testing

@testable import bae

/// A hosted window's lifetime across opens.
@MainActor
@Suite("Hosting window")
struct HostingWindowControllerTests {
    @Test("closing the window tells its owner, which lets it go")
    func closingLetsTheWindowGo() async throws {
        let owner = WindowOwner()
        weak var released: HostingWindowController?
        autoreleasepool {
            let controller = HostingWindowController(
                content: Text(verbatim: "Window"),
                contentSize: CGSize(width: 200, height: 100),
                sizing: [],
                bridging: [],
                onClose: { owner.controller = nil }
            )
            owner.controller = controller
            released = controller
            Wait.announceRelease(of: controller)
        }
        #expect(released != nil)

        autoreleasepool {
            owner.controller?.window?.close()
        }

        #expect(owner.controller == nil)
        try await Wait.until { released == nil }
    }
}

/// Holds a window's controller while it is open, as the app delegate does.
@MainActor
final class WindowOwner {
    var controller: HostingWindowController?
}
