import BaeKit
import SwiftUI

/// An icon button boxed in the push button's fill, so it reads as a button
/// among text: the design system's boxed `MacIconButton`. The glyph is primary
/// text.
struct BoxedIconButtonStyle: ButtonStyle {
    enum Size {
        /// A control inside a line of text, such as a transfer's Pause: 20
        /// points square, with the control radius.
        case small
        /// A button on its own, as tall as the hit target: 28 points square,
        /// with the control radius.
        case control
        /// The button back to the page before, at a page's top left: a
        /// circle as tall as a control.
        case back

        var side: CGFloat {
            switch self {
            case .small: 20
            case .control: ThemeSize.hitTarget
            // The design system's `size-control`.
            case .back: 24
            }
        }

        var cornerRadius: CGFloat {
            switch self {
            case .small, .control: ThemeRadius.control
            case .back: side / 2
            }
        }
    }

    let size: Size

    var focused: AccentFocusButtonStyle<Self> {
        accentFocus(cornerRadius: size.cornerRadius)
    }

    func makeBody(configuration: Configuration) -> some View {
        BoxedIconFace(size: size, isPressed: configuration.isPressed) {
            configuration.label
        }
    }
}

/// The face of a boxed icon button around `label`: what the style draws, for
/// a button that is a menu's label and has no button style of its own.
struct BoxedIconFace<Label: View>: View {
    let size: BoxedIconButtonStyle.Size
    let isPressed: Bool
    @ViewBuilder
    let label: Label

    @Environment(\.colorScheme)
    private var scheme
    @State
    private var hovering = false

    var body: some View {
        let dark = scheme == .dark
        label
            .foregroundStyle(Theme.textPrimary)
            .frame(width: size.side, height: size.side)
            .background(
                RoundedRectangle(cornerRadius: size.cornerRadius)
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
                RoundedRectangle(cornerRadius: size.cornerRadius)
                    .strokeBorder(
                        dark ? Color.clear : Color.black.opacity(0.12),
                        lineWidth: 0.5
                    )
                RoundedRectangle(cornerRadius: size.cornerRadius)
                    .fill(Theme.textPrimary.opacity(hovering ? 0.15 : 0))
            }
            .opacity(isPressed ? 0.6 : 1)
            .contentShape(Rectangle())
            .onHover { hovering = $0 }
    }
}
