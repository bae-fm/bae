import AppKit
import SwiftUI
import Testing

@testable import bae

/// Where `TrafficLightOffset` puts a window's close, minimize and zoom
/// buttons: their usual place, moved by the offset, as the window opens and
/// after it is resized.
@MainActor
@Suite("TrafficLightOffset")
struct TrafficLightOffsetTests {
    @Test("the buttons sit the offset away from their usual place")
    func buttonsMoveByTheOffset() async throws {
        let usual = Self.window(hosting: Color.clear)
        let offset = Self.window(
            hosting: Color.clear.modifier(
                TrafficLightOffset(xOffset: Self.xOffset, yOffset: Self.yOffset)
            )
        )
        defer {
            usual.close()
            offset.close()
        }

        try await Wait.untilLaidOut(try #require(offset.contentView)) {
            try Self.buttonOrigins(in: offset)
                == Self.movedOrigins(Self.buttonOrigins(in: usual))
        }
    }

    @Test("a resized window keeps the buttons the offset away")
    func buttonsStayAfterResize() async throws {
        let usual = Self.window(hosting: Color.clear)
        let offset = Self.window(
            hosting: Color.clear.modifier(
                TrafficLightOffset(xOffset: Self.xOffset, yOffset: Self.yOffset)
            )
        )
        defer {
            usual.close()
            offset.close()
        }
        try await Wait.untilLaidOut(try #require(offset.contentView)) {
            try Self.buttonOrigins(in: offset)
                == Self.movedOrigins(Self.buttonOrigins(in: usual))
        }

        for window in [usual, offset] {
            window.setContentSize(CGSize(width: 1_000, height: 640))
            Self.layOut(window)
        }
        try await Wait.untilLaidOut(try #require(offset.contentView)) {
            try Self.buttonOrigins(in: offset)
                == Self.movedOrigins(Self.buttonOrigins(in: usual))
        }
    }

    @Test("buttons AppKit puts back are moved again, once")
    func buttonsAreMovedAgainAfterAppKitPutsThemBack() async throws {
        let usual = Self.window(hosting: Color.clear)
        let offset = Self.window(
            hosting: Color.clear.modifier(
                TrafficLightOffset(xOffset: Self.xOffset, yOffset: Self.yOffset)
            )
        )
        defer {
            usual.close()
            offset.close()
        }
        let expected = Self.movedOrigins(try Self.buttonOrigins(in: usual))
        try await Wait.untilLaidOut(try #require(offset.contentView)) {
            try Self.buttonOrigins(in: offset) == expected
        }

        // AppKit lays the title bar out again as the window becomes key or
        // updates, which puts each button back at its usual place.
        for type: NSWindow.ButtonType in [
            .closeButton, .miniaturizeButton, .zoomButton,
        ] {
            let button = try #require(offset.standardWindowButton(type))
            let usualOrigin = try #require(usual.standardWindowButton(type))
                .frame.origin
            button.setFrameOrigin(usualOrigin)
        }
        offset.update()
        try await Wait.untilLaidOut(try #require(offset.contentView)) {
            try Self.buttonOrigins(in: offset) == expected
        }

        // Nothing put them back, so another update leaves them where they are.
        offset.update()
        offset.update()
        #expect(try Self.buttonOrigins(in: offset) == expected)
    }

    private static let xOffset: CGFloat = 6
    private static let yOffset: CGFloat = 7

    /// A window with the main window's hidden title bar hosting `view`, laid
    /// out but never ordered in, so nothing shows on the screen.
    private static func window<V: View>(hosting view: V) -> NSWindow {
        let window = NSWindow(
            contentRect: CGRect(x: 0, y: 0, width: 900, height: 600),
            styleMask: [
                .titled, .closable, .miniaturizable, .resizable,
                .fullSizeContentView,
            ],
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.contentView = NSHostingView(rootView: view)
        layOut(window)
        return window
    }

    /// Lay out the whole window, title bar included.
    private static func layOut(_ window: NSWindow) {
        window.contentView?.superview?.layoutSubtreeIfNeeded()
    }

    private static func buttonOrigins(in window: NSWindow) throws -> [CGPoint] {
        layOut(window)
        let buttons: [NSWindow.ButtonType] = [
            .closeButton, .miniaturizeButton, .zoomButton,
        ]
        return try buttons.map {
            try #require(window.standardWindowButton($0)).frame.origin
        }
    }

    private static func movedOrigins(_ origins: [CGPoint]) -> [CGPoint] {
        origins.map { CGPoint(x: $0.x + xOffset, y: $0.y - yOffset) }
    }
}
