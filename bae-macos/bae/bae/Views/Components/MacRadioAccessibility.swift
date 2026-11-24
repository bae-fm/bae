import AppKit
import SwiftUI

/// The accessibility element of a radio choice, as tall and wide as the view
/// it overlays: a radio button that reads `label`, is selected when `selected`
/// and makes the choice when VoiceOver presses it. It takes no clicks, so the
/// view under it keeps them.
struct MacRadioAccessibility: NSViewRepresentable {
    let label: String
    let selected: Bool
    let onPress: () -> Void

    func makeNSView(context: Context) -> RadioElementView {
        RadioElementView()
    }

    func updateNSView(_ view: RadioElementView, context: Context) {
        view.label = label
        view.selected = selected
        view.onPress = onPress
    }

    final class RadioElementView: NSView {
        var label = ""
        var selected = false
        var onPress: () -> Void = {}

        override func isAccessibilityElement() -> Bool { true }

        override func accessibilityRole() -> NSAccessibility.Role? {
            .radioButton
        }

        override func accessibilityLabel() -> String? { label }

        override func accessibilityValue() -> Any? { selected ? 1 : 0 }

        override func accessibilityPerformPress() -> Bool {
            onPress()
            return true
        }

        override func hitTest(_ point: NSPoint) -> NSView? { nil }
    }
}
