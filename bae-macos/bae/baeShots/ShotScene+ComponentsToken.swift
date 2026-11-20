import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacToken`, a name pattern's value as a chip, on its own as its
    /// preview draws it: a value, and a longer value.
    static let tokenScenes: [ShotScene] = [
        component("mac.component.token.value", height: 40) {
            TokenShot(label: "Artist")
        },
        component("mac.component.token.longer-value", height: 40) {
            TokenShot(label: "Catalog number")
        },
    ]
}

/// A `MacToken` drawn outside a field, as its preview draws it, and read by
/// VoiceOver as its value's name, as the field reads it.
private struct TokenShot: NSViewRepresentable {
    let label: String

    @Environment(\.accentChoice)
    private var accent

    func makeNSView(context _: Context) -> TokenView {
        TokenView()
    }

    func updateNSView(_ view: TokenView, context _: Context) {
        view.token = MacToken(label, accent: accent.appKitColor)
        view.setAccessibilityValue(label)
    }

    func sizeThatFits(
        _: ProposedViewSize,
        nsView view: TokenView,
        context _: Context
    ) -> CGSize? {
        view.token?.cellSize()
    }

    final class TokenView: NSView {
        var token: MacToken? {
            didSet { needsDisplay = true }
        }

        override func draw(_: NSRect) {
            token?.draw(withFrame: bounds, in: self)
        }

        override func isAccessibilityElement() -> Bool { true }

        override func accessibilityRole() -> NSAccessibility.Role? {
            .staticText
        }
    }
}
