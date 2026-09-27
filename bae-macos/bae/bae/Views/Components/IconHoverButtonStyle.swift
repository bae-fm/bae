import BaeKit
import SwiftUI

/// Hover treatment for icon-only buttons; a glyph color set on the label wins
/// over this one.
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
                        .fill(hovering ? Theme.hover : Color.clear),
                )
                .opacity(configuration.isPressed ? 0.6 : 1)
                .onHover { hovering = $0 }
        }
    }
}

#if DEBUG
    #Preview("Icon Hover Button Style") {
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
