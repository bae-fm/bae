import AppKit
import BaeKit
import SwiftUI
import Synchronization
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

    @Test(
        "a move that cannot save its display reports the failure and keeps the saved frame"
    )
    func reportsAnUnidentifiedDisplayWithoutLosingTheSavedFrame() throws {
        let screen = try #require(NSScreen.screens.first)
        let name = "ShotWindowPlacementTests.\(UUID())"
        let defaults = try #require(UserDefaults(suiteName: name))
        defer {
            NSWindow.removeFrame(usingName: name)
            defaults.removePersistentDomain(forName: name)
        }
        let reports = Mutex(0)
        var alerts: [BridgeError] = []
        let identifier = UUID()
        var identifiesDisplay = true
        let autosave = WindowFrameAutosave(
            name: name,
            defaults: defaults,
            bugReporter: PlacementBugReporter { _, _ in
                reports.withLock { $0 += 1 }
            },
            showError: { alerts.append($0) },
            identifyDisplay: { _ in identifiesDisplay ? identifier : nil }
        )
        let placement = WindowFrameAutosave.Opening(
            autosave: autosave,
            preferredScreen: screen,
            screens: [screen]
        )
        let size = CGSize(width: 500, height: 320)
        let saved = try autoreleasepool {
            let first = Self.controller(size: size, placement: placement)
            let window = try #require(first.window)
            first.showWindow(nil)
            window.setFrameOrigin(
                NSPoint(
                    x: screen.visibleFrame.minX + 40,
                    y: screen.visibleFrame.minY + 60
                )
            )
            let goodFrame = window.frame
            identifiesDisplay = false
            window.setFrameOrigin(
                NSPoint(
                    x: screen.visibleFrame.minX + 70,
                    y: screen.visibleFrame.minY + 90
                )
            )
            #expect(window.frame != goodFrame)
            #expect(reports.withLock { $0 } > 0)
            #expect(!alerts.isEmpty)
            window.close()
            return goodFrame
        }

        identifiesDisplay = true
        let reportedBeforeReopening = alerts.count
        let reopened = Self.controller(size: size, placement: placement)
        let window = try #require(reopened.window)
        defer { window.close() }
        reopened.showWindow(nil)
        #expect(window.frame == saved)
        #expect(alerts.count == reportedBeforeReopening)
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
    private let handler: @Sendable (String, String) -> Void

    init(
        _ handler: @escaping @Sendable (String, String) -> Void = {
            Issue.record("Unexpected placement failure during \($0): \($1)")
        }
    ) {
        self.handler = handler
    }

    func report(operation: String, detail: String) {
        handler(operation, detail)
    }
}
