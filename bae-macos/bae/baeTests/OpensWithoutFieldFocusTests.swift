import AppKit
import SwiftUI
import Testing

@testable import bae

/// AppKit gives the keyboard to the first field of a window's tree when the
/// window becomes key, which draws that field's focus ring where the boards
/// draw a window at rest; a window that opens without field focus leaves the
/// keyboard with a plain view.
@MainActor
@Suite("Opens without field focus")
struct OpensWithoutFieldFocusTests {
    /// A borderless window that can be key without the app being active, as
    /// the shot renderer's windows are.
    private final class KeyPanel: NSPanel {
        override var canBecomeKey: Bool { true }
    }

    /// Past every display, where no pointer hovers the window.
    private var offscreenOrigin: NSPoint {
        let right = NSScreen.screens.map(\.frame.maxX).max() ?? 0
        return NSPoint(x: right + 1_000, y: 0)
    }

    /// The view that holds the keyboard once a window showing `view` is key.
    private func keyboardHolder(_ view: some View) async throws -> NSResponder {
        let size = NSSize(width: 240, height: 80)
        let host = NSHostingView(
            rootView: view.frame(width: size.width, height: size.height)
        )
        host.frame = NSRect(origin: .zero, size: size)
        let panel = KeyPanel(
            contentRect: NSRect(origin: offscreenOrigin, size: size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isReleasedWhenClosed = false
        panel.animationBehavior = .none
        panel.contentView = host
        defer { panel.close() }
        panel.makeKeyAndOrderFront(nil)
        try await Wait.until(
            notices: [Wait.Notice(NSWindow.didBecomeKeyNotification, of: panel)]
        ) {
            panel.isKeyWindow
        }
        try await SnapshotTestSupport.settle(host)
        return try #require(panel.firstResponder)
    }

    @Test("a field takes the keyboard when its window becomes key")
    func fieldTakesTheKeyboard() async throws {
        let holder = try await keyboardHolder(
            TextField("Search", text: .constant(""))
        )
        #expect(holder is NSText, "held by \(holder)")
    }

    @Test("a window that opens without field focus leaves the field unfocused")
    func fieldLeftUnfocused() async throws {
        let holder = try await keyboardHolder(
            TextField("Search", text: .constant(""))
                .opensWithoutFieldFocus()
        )
        #expect(!(holder is NSText), "held by \(holder)")
        #expect(!(holder is NSTextField), "held by \(holder)")
    }
}
