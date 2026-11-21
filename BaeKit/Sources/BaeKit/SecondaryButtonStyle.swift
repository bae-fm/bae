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

    /// The corner radius to draw with in place of the control size's own.
    private let cornerRadius: CGFloat?

    /// Whether the fill is the primary text colour at 12%, so the button
    /// reads on a filled notice of every tone.
    private let tinted: Bool

    public init(cornerRadius: CGFloat? = nil, tinted: Bool = false) {
        self.cornerRadius = cornerRadius
        self.tinted = tinted
    }

    public func makeBody(configuration: Configuration) -> some View {
        Face(
            configuration: configuration,
            height: metrics.height,
            padding: metrics.padding,
            shape: shape,
            dark: colorScheme == .dark,
            enabled: isEnabled,
            tinted: tinted,
            size: controlSize
        )
    }

    private var metrics: MacButtonMetrics {
        MacButtonMetrics(controlSize: controlSize)
    }

    private var shape: AnyShape {
        if let cornerRadius {
            return AnyShape(RoundedRectangle(cornerRadius: cornerRadius))
        }
        return metrics.shape
    }

    private struct Face: View {
        let configuration: ButtonStyleConfiguration
        let height: CGFloat
        let padding: CGFloat
        let shape: AnyShape
        let dark: Bool
        let enabled: Bool
        let tinted: Bool
        let size: ControlSize

        var body: some View {
            configuration.label
                .buttonWords(size)
                .foregroundStyle(Theme.textPrimary)
                .padding(.horizontal, padding)
                .frame(height: height)
                .background(fill, in: shape)
                .macButtonStateLayers(
                    shape,
                    isPressed: configuration.isPressed,
                    enabled: enabled
                )
                .overlay {
                    shape.stroke(
                        SecondaryButtonStyle.lightEdge,
                        lineWidth: 0.5
                    )
                    .opacity(dark ? 0 : 1)
                }
                .contentShape(shape)
                .opacity(enabled ? 1 : ThemeOpacity.off)
        }

        private var fill: AnyShapeStyle {
            if tinted { return AnyShapeStyle(Theme.textPrimary.opacity(0.12)) }
            return dark ? AnyShapeStyle(Theme.tile) : AnyShapeStyle(Color.white)
        }
    }
}
