import SwiftUI

/// The design's secondary button: the control fill (the tone's tile in dark,
/// white in light with a thin edge, as macOS draws its push buttons) under
/// the primary text colour, as high as the control size says. Hover and
/// pressed lay the design's overlays over the fill.
public struct SecondaryButtonStyle: ButtonStyle {
    @Environment(\.controlSize)
    private var controlSize
    @Environment(\.colorScheme)
    private var colorScheme
    @Environment(\.isEnabled)
    private var isEnabled

    /// The edge a white button needs on a pale sheet; none in dark, where the
    /// fill is lighter than everything around it.
    private static let lightEdge = Color.black.opacity(0.12)

    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        Face(
            configuration: configuration,
            height: height,
            shape: shape,
            dark: colorScheme == .dark,
            enabled: isEnabled
        )
    }

    private var height: CGFloat {
        switch controlSize {
        case .mini, .small: 20
        case .large, .extraLarge: 28
        default: 24
        }
    }

    private var shape: AnyShape {
        switch controlSize {
        case .large, .extraLarge: AnyShape(Capsule())
        case .mini, .small: AnyShape(RoundedRectangle(cornerRadius: 4))
        default: AnyShape(RoundedRectangle(cornerRadius: 5))
        }
    }

    private struct Face: View {
        let configuration: ButtonStyleConfiguration
        let height: CGFloat
        let shape: AnyShape
        let dark: Bool
        let enabled: Bool

        @State
        private var hovering = false

        var body: some View {
            configuration.label
                .foregroundStyle(.primary)
                .padding(.horizontal, 12)
                .frame(height: height)
                .background(fill, in: shape)
                .overlay {
                    if configuration.isPressed {
                        shape.fill(Theme.pressed)
                    }
                    else if hovering && enabled {
                        shape.fill(Theme.hover)
                    }
                }
                .overlay {
                    if !dark {
                        shape.stroke(
                            SecondaryButtonStyle.lightEdge,
                            lineWidth: 0.5
                        )
                    }
                }
                .contentShape(shape)
                .opacity(enabled ? 1 : ThemeOpacity.off)
                .onHover { hovering = $0 }
        }

        private var fill: AnyShapeStyle {
            dark ? AnyShapeStyle(Theme.tile) : AnyShapeStyle(Color.white)
        }
    }
}
