import BaeKit
import SwiftUI

/// A glyph button of the hit-target size: secondary at rest, a hover fill, and
/// the accent on a tint while `active` (shuffle on, the queue shown, casting).
/// `iconSize` and `hitSize` set the glyph and the button, and `faint` draws
/// the glyph in tertiary text, for a button that should sit back. `boxed`
/// draws the glyph on the push button's fill. `label`
/// names it to VoiceOver and, with no `help`, to the pointer.
struct MacIconButton: View {
    let symbol: String
    let label: LocalizedStringKey
    var help: LocalizedStringKey?
    var active = false
    var faint = false
    var iconSize: ThemeIcon = .large
    var hitSize = ThemeSize.hitTarget
    var boxed: BoxedIconButtonStyle.Size?
    var role: ButtonRole?
    let action: () -> Void

    static func remove(_ name: String, action: @escaping () -> Void) -> Self {
        Self(
            symbol: "xmark.circle.fill",
            label: "Remove \(name)",
            faint: true,
            iconSize: .medium,
            hitSize: BoxedIconButtonStyle.Size.small.side,
            role: .destructive,
            action: action
        )
    }

    var body: some View {
        let button = Button(role: role, action: action) {
            Image(systemName: symbol)
                .themeIcon(iconSize)
                .frame(
                    width: boxed == nil ? hitSize : nil,
                    height: boxed == nil ? hitSize : nil
                )
                .contentShape(Rectangle())
        }
        Group {
            if let boxed {
                button.buttonStyle(
                    BoxedIconButtonStyle(size: boxed)
                        .accentFocus(cornerRadius: boxed.cornerRadius)
                )
            }
            else {
                button.buttonStyle(
                    IconHoverButtonStyle(isOn: active, faint: faint)
                        .accentFocus()
                )
            }
        }
        .help(help ?? label)
        .accessibilityLabel(label)
    }
}
