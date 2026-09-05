import BaeKit
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
                        .fill(Color.primary.opacity(hovering ? 0.06 : 0)),
                )
                .opacity(configuration.isPressed ? 0.6 : 1)
                .onHover { hovering = $0 }
        }
    }
}

#if DEBUG
    #Preview("Icon Hover Button Style") {
        // Icon-only buttons hosting the style — hover a glyph to see the rounded
        // fill and the secondary→primary step.
        HStack(spacing: 16) {
            Button {
            } label: {
                Image(systemName: "backward.fill")
            }
            Button {
            } label: {
                Image(systemName: "play.fill")
            }
            Button {
            } label: {
                Image(systemName: "forward.fill")
            }
            Button {
            } label: {
                Image(systemName: "gearshape")
            }
        }
        .buttonStyle(IconHoverButtonStyle())
        .font(.system(size: 16, weight: .semibold))
        .padding(28)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
