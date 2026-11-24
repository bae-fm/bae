import AppKit
import SwiftUI

extension View {
    /// The window this view is in opens with no text field holding the
    /// keyboard, as the boards draw a window at rest. AppKit gives the
    /// keyboard to the window's first field when the window becomes key,
    /// unless the window names another view to take it first: a plain view
    /// here, so the first field takes it only when the person asks.
    func opensWithoutFieldFocus() -> some View {
        background(FirstToTakeTheKeyboard())
    }
}

/// A view with nothing to type into that the window names as the first to
/// take the keyboard.
private struct FirstToTakeTheKeyboard: NSViewRepresentable {
    func makeNSView(context _: Context) -> Anchor {
        Anchor()
    }

    func updateNSView(_: Anchor, context _: Context) {}

    final class Anchor: NSView {
        override var acceptsFirstResponder: Bool { true }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            window?.initialFirstResponder = self
        }
    }
}
