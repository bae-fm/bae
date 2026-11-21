import SwiftUI

extension View {
    /// A small button's words are 11 points, as the design draws them; a
    /// button of another size takes the words' font from where it sits.
    @ViewBuilder
    func buttonWords(_ size: ControlSize) -> some View {
        switch size {
        case .mini, .small: themeText(.cell)
        default: self
        }
    }
}

/// The design's primary button: the palette's readable fill under white
/// words, as high as the control size says. Hover and pressed lay the
/// design's overlays over the fill, and off dims the button as one piece.
public struct PrimaryButtonStyle: ButtonStyle {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.controlSize)
    private var controlSize
    @Environment(\.isEnabled)
    private var isEnabled

    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        Face(
            configuration: configuration,
            fill: accent.buttonColor,
            height: metrics.height,
            padding: metrics.padding,
            shape: metrics.shape,
            enabled: isEnabled,
            size: controlSize
        )
    }

    private var metrics: MacButtonMetrics {
        MacButtonMetrics(controlSize: controlSize)
    }

    private struct Face: View {
        let configuration: ButtonStyleConfiguration
        let fill: Color
        let height: CGFloat
        let padding: CGFloat
        let shape: AnyShape
        let enabled: Bool
        let size: ControlSize

        var body: some View {
            configuration.label
                .buttonWords(size)
                .foregroundStyle(Theme.onFill)
                .padding(.horizontal, padding)
                .frame(height: height)
                .background(fill, in: shape)
                .macButtonStateLayers(
                    shape,
                    isPressed: configuration.isPressed,
                    enabled: enabled
                )
                .contentShape(shape)
                .compositingGroup()
                .opacity(enabled ? 1 : ThemeOpacity.off)
        }
    }
}
