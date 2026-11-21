import SwiftUI

/// A button of words with no fill, in the box a regular button has: 24 points
/// high with 12 points either side of the words (20 and 8 when small, 28 and
/// 14 when large), so the box is what the accessibility frame and a click
/// reach. The words take the accent, or the colour the label sets.
public struct MacBorderlessButtonStyle: ButtonStyle {
    @Environment(\.controlSize)
    private var controlSize
    @Environment(\.isEnabled)
    private var isEnabled

    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        Face(
            configuration: configuration,
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
        let height: CGFloat
        let padding: CGFloat
        let shape: AnyShape
        let enabled: Bool
        let size: ControlSize

        var body: some View {
            configuration.label
                .buttonWords(size)
                .foregroundStyle(Theme.accent)
                .padding(.horizontal, padding)
                .frame(height: height)
                .macButtonStateLayers(
                    shape,
                    isPressed: configuration.isPressed,
                    enabled: enabled
                )
                .contentShape(shape)
                .opacity(enabled ? 1 : ThemeOpacity.off)
        }
    }
}

extension PrimitiveButtonStyle
where Self == AccentFocusButtonStyle<MacBorderlessButtonStyle> {
    /// `MacBorderlessButtonStyle`, wearing the accent's focus ring.
    public static var macBorderless: Self {
        MacBorderlessButtonStyle().accentFocus()
    }
}
