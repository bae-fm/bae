import BaeKit
import SwiftUI

/// A glyph button of the hit-target size: secondary at rest, a hover fill, and
/// the accent on a tint while `active` (shuffle on, the queue shown, casting).
/// `label` names it to VoiceOver and, with no `help`, to the pointer.
struct MacIconButton: View {
    let symbol: String
    let label: LocalizedStringKey
    var help: LocalizedStringKey?
    var active = false
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
        .buttonStyle(IconHoverButtonStyle(isOn: active).accentFocus())
        .help(help ?? label)
        .accessibilityLabel(label)
    }
}
