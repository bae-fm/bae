import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct ShotWindowPlacementTests {
    enum Reopen: CaseIterable {
        case sameSize, changedSize, missingDisplayAndAppKitFrame
    }

    @Test(
        "a content-sized window centers on first open and restores after a move",
        arguments: Reopen.allCases
    )
    func restoresMovedWindow(reopen: Reopen) async throws {
        let movedScreen = try await ShotDisplay.shared().screen()
        let screens = NSScreen.screens
        let firstScreen = try #require(screens.first)
        try #require(movedScreen.displayNumber != firstScreen.displayNumber)
        let name = "ShotWindowPlacementTests.\(UUID())"
        let defaults = try #require(UserDefaults(suiteName: name))
        defer {
            NSWindow.removeFrame(usingName: name)
            defaults.removePersistentDomain(forName: name)
        }
        let initialSize = CGSize(width: 500, height: 320)
        let autosave = WindowFrameAutosave(
            name: name,
            defaults: defaults,
            bugReporter: PlacementBugReporter(),
            showError: {
                Issue.record(
                    "Unexpected placement error \(String(reflecting: $0))"
                )
            }
        )
        func makeController(
            size: CGSize,
            on screens: [NSScreen]
        ) -> HostingWindowController {
            let placement = WindowFrameAutosave.Opening(
                autosave: autosave,
                preferredScreen: firstScreen,
                screens: screens
            )
            return Self.controller(size: size, placement: placement)
        }

        let moved = try autoreleasepool {
            try Self.move(
                makeController(size: initialSize, on: screens),
                size: initialSize,
                from: firstScreen,
                to: movedScreen
            )
        }

        let reopenedSize =
            reopen == .changedSize
            ? CGSize(width: 570, height: 380) : initialSize
        let disconnected = reopen == .missingDisplayAndAppKitFrame
        if disconnected { NSWindow.removeFrame(usingName: name) }
        let connected = disconnected ? [firstScreen] : screens
        let reopened = makeController(size: reopenedSize, on: connected)
        let window = try #require(reopened.window)
        defer { window.close() }
        reopened.showWindow(nil)
        #expect(window.frame.size == reopenedSize)
        if disconnected {
            Self.expectCentered(window.frame, on: firstScreen)
        }
        else {
            #expect(window.frame.minX == moved.minX)
            #expect(window.frame.maxY == moved.maxY)
            #expect(window.screen == movedScreen)
        }
    }

    private static func controller(
        size: CGSize,
        placement: WindowFrameAutosave.Opening
    ) -> HostingWindowController {
        HostingWindowController(
            content: Color.clear.frame(width: size.width, height: size.height),
            contentSize: nil,
            sizing: [.minSize, .maxSize],
            bridging: [],
            framePlacement: placement,
            configureWindow: { window in
                window.showOnlyWindowButtons()
                window.animationBehavior = .none
            },
            onClose: {}
        )
    }

    private static func expectCentered(_ frame: CGRect, on screen: NSScreen) {
        let room = screen.visibleFrame
        #expect(room.contains(frame))
        #expect(abs(frame.midX - room.midX) <= 1)
        #expect(abs(frame.midY - room.midY) <= 1)
    }

    private static func move(
        _ controller: HostingWindowController,
        size: CGSize,
        from firstScreen: NSScreen,
        to movedScreen: NSScreen
    ) throws -> CGRect {
        let window = try #require(controller.window)
        defer { window.close() }
        controller.showWindow(nil)
        #expect(window.frame.size == size)
        expectCentered(window.frame, on: firstScreen)
        window.setFrameOrigin(
            CGPoint(
                x: movedScreen.visibleFrame.minX + 40,
                y: movedScreen.visibleFrame.minY + 110
            )
        )
        return window.frame
    }
}

private final class PlacementBugReporter: BridgeBugReporter {
    func report(operation: String, detail: String) {
        Issue.record(
            "Unexpected placement failure during \(operation): \(detail)"
        )
    }
}
