import BaeKit
import SwiftUI

/// Hover treatment for icon-only buttons; a glyph color set on the label wins
/// over this one. A button that is on (shuffle, repeat, an open queue, an
/// active cast) draws its glyph in the accent on an accent tint, which hover
/// leaves as it is.
struct IconHoverButtonStyle: ButtonStyle {
    var isOn = false

    func makeBody(configuration: Configuration) -> some View {
        Hovering(configuration: configuration, isOn: isOn)
    }

    private struct Hovering: View {
        let configuration: Configuration
        let isOn: Bool
        @State
        private var hovering = false

        var body: some View {
            configuration.label
                .foregroundStyle(
                    isOn
                        ? Theme.accent
                        : hovering ? Color.primary : Color.secondary
                )
                .background(
                    RoundedRectangle(cornerRadius: ThemeRadius.control)
                        .fill(
                            isOn
                                ? Theme.accentSoft
                                : hovering ? Theme.hover : Color.clear
                        ),
                )
                .opacity(configuration.isPressed ? 0.6 : 1)
                .onHover { hovering = $0 }
        }
    }
}
