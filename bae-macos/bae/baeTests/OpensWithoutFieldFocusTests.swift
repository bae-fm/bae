import AppKit
import SwiftUI
import Testing

@testable import bae

/// A window that opens puts the keyboard in the first field of its tree,
/// which draws that field's focus ring where the boards draw a window at
/// rest; a window that opens without field focus names a plain view as the
/// first to take the keyboard.
@MainActor
@Suite("Opens without field focus")
struct OpensWithoutFieldFocusTests {
    @Test("the window's first responder to be is not a text field")
    func firstResponderToBeIsNoField() async throws {
        let size = NSSize(width: 240, height: 80)
        try await SnapshotTestSupport.withHostedWindow(
            TextField("Search", text: .constant(""))
                .opensWithoutFieldFocus()
                .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let first = try #require(
                window.initialFirstResponder,
                "no initial first responder named"
            )
            #expect(!(first is NSText), "named \(first)")
            #expect(first.acceptsFirstResponder)
        }
    }
}
