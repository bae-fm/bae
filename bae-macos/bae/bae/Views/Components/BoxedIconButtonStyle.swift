import BaeKit
import SwiftUI

/// An icon button boxed in the push button's fill, so it reads as a button
/// among text: the design system's boxed `MacIconButton`. The box is 20
/// points square, with the control radius; the glyph is primary text.
struct BoxedIconButtonStyle: ButtonStyle {
    enum Size {
        /// A control inside a line of text, such as a transfer's Pause.
        case small

        var side: CGFloat {
            switch self {
            case .small: 20
            }
        }
    }

    let size: Size

    func makeBody(configuration: Configuration) -> some View {
        Box(configuration: configuration, size: size)
    }

    private struct Box: View {
        let configuration: Configuration
        let size: Size
        @Environment(\.colorScheme)
        private var scheme
        @State
        private var hovering = false

        var body: some View {
            let dark = scheme == .dark
            configuration.label
                .foregroundStyle(Color.primary)
                .frame(width: size.side, height: size.side)
                .background(
                    RoundedRectangle(cornerRadius: ThemeRadius.control)
                        // The push button's fill: the tile in dark, white in
                        // light, where a ring and a drop stand it off a pale
                        // sheet.
                        .fill(
                            dark
                                ? AnyShapeStyle(Theme.tile)
                                : AnyShapeStyle(Color.white)
                        )
                        .shadow(
                            color: dark ? .clear : Color.black.opacity(0.12),
                            radius: 0.5,
                            y: 0.5
                        )
                )
                .overlay {
                    RoundedRectangle(cornerRadius: ThemeRadius.control)
                        .strokeBorder(
                            dark ? Color.clear : Color.black.opacity(0.12),
                            lineWidth: 0.5
                        )
                    RoundedRectangle(cornerRadius: ThemeRadius.control)
                        .fill(Color.primary.opacity(hovering ? 0.15 : 0))
                }
                .opacity(configuration.isPressed ? 0.6 : 1)
                .contentShape(Rectangle())
                .onHover { hovering = $0 }
        }
    }
}
