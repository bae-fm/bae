import AppKit
import SwiftUI
import Testing

@testable import bae

/// A hosted window's frame and lifetime across opens.
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

    @Test(
        "a content-sized window centers on first open and restores after a move"
    )
    func restoresMovedWindow() throws {
        let firstScreen = try #require(NSScreen.screens.first)
        let movedScreen = try #require(NSScreen.screens.last)
        let name = "HostingWindowControllerTests.\(UUID())"
        defer { NSWindow.removeFrame(usingName: name) }

        func makeController() -> HostingWindowController {
            HostingWindowController(
                content: Color.clear.frame(width: 500, height: 320),
                contentSize: nil,
                sizing: [.minSize, .maxSize],
                bridging: [],
                frameAutosave: .init(name: name, firstScreen: firstScreen),
                configureWindow: { $0.animationBehavior = .none },
                onClose: {}
            )
        }

        let moved = try autoreleasepool {
            let controller = makeController()
            let window = try #require(controller.window)
            defer { window.close() }
            controller.showWindow(nil)

            #expect(
                window.contentView?.frame.size
                    == CGSize(width: 500, height: 320)
            )
            let room = firstScreen.visibleFrame
            #expect(room.contains(window.frame))
            #expect(abs(window.frame.midX - room.midX) <= 1)
            #expect(abs(window.frame.midY - room.midY) <= 1)

            window.setFrameOrigin(
                CGPoint(
                    x: movedScreen.visibleFrame.minX + 40,
                    y: movedScreen.visibleFrame.minY + 60
                )
            )
            return window.frame
        }

        let reopened = makeController()
        let window = try #require(reopened.window)
        defer { window.close() }
        reopened.showWindow(nil)
        #expect(window.frame == moved)
        #expect(window.screen == movedScreen)
    }
}

/// Holds a window's controller while it is open, as the app delegate does.
@MainActor
final class WindowOwner {
    var controller: HostingWindowController?
}
