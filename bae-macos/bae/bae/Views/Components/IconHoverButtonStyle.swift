import SwiftUI

/// Hover treatment shared by icon-only buttons (the now-playing bar's transport
/// and utility glyphs, the title bar's gear): a rounded subtle fill while the
/// pointer is over it, and the glyph stepping from the secondary color toward
/// the primary. Buttons that carry an active-state tint set their own glyph
/// color on the label, which wins over this base.
struct IconHoverButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Hovering(configuration: configuration)
    }

    private struct Hovering: View {
        let configuration: Configuration
        @State
        private var hovering = false

        var body: some View {
            configuration.label
                .foregroundStyle(hovering ? Color.primary : Color.secondary)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color.white.opacity(hovering ? 0.06 : 0)),
                )
                .opacity(configuration.isPressed ? 0.6 : 1)
                .onHover { hovering = $0 }
        }
    }
}
