import BaeKit
import SwiftUI

/// A glyph button of the hit-target size: secondary at rest, a hover fill, and
/// the accent on a tint while `active` (shuffle on, the queue shown, casting).
/// `iconSize` and `hitSize` set the glyph and the button, and `faint` draws
/// the glyph in tertiary text, for a button that should sit back. `label`
/// names it to VoiceOver and, with no `help`, to the pointer.
struct MacIconButton: View {
    let symbol: String
    let label: LocalizedStringKey
    var help: LocalizedStringKey?
    var active = false
    var faint = false
    var iconSize: ThemeIcon = .large
    var hitSize = ThemeSize.hitTarget
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .themeIcon(iconSize)
                .frame(width: hitSize, height: hitSize)
                .contentShape(Rectangle())
        }
        .buttonStyle(
            IconHoverButtonStyle(isOn: active, faint: faint).accentFocus()
        )
        .help(help ?? label)
        .accessibilityLabel(label)
    }
}
