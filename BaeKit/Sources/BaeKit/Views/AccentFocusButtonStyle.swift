import SwiftUI

/// `.borderless`, with the system's focus ring replaced by the accent's
/// (`accentFocusRing`). The style keeps its own look, keyboard handling,
/// pressed state and VoiceOver role.
public struct AccentFocusBorderlessButtonStyle: PrimitiveButtonStyle {
    public func makeBody(configuration: Configuration) -> some View {
        Button(configuration)
            .buttonStyle(.borderless)
            .accentFocusRing(cornerRadius: ThemeRadius.control)
    }
}

/// A button style of bae's own with the system's focus ring replaced by the
/// accent's.
public struct AccentFocusButtonStyle<Base: ButtonStyle>: PrimitiveButtonStyle {
    let base: Base
    let cornerRadius: CGFloat

    public func makeBody(configuration: Configuration) -> some View {
        Button(configuration)
            .buttonStyle(base)
            .accentFocusRing(cornerRadius: cornerRadius)
    }
}

/// The look of SwiftUI's `.plain` button style, with bae's off opacity while
/// the button is disabled: `.plain` dims a disabled button to its own 0.5,
/// where every other bae control dims to `ThemeOpacity.off`.
public struct PlainButtonLook: ButtonStyle {
    @Environment(\.isEnabled)
    private var isEnabled
    private let dimsWhenDisabled: Bool

    /// With `dimsWhenDisabled` false, the button draws as enabled while it
    /// is disabled, for one that something drawn over it covers.
    public init(dimsWhenDisabled: Bool = true) {
        self.dimsWhenDisabled = dimsWhenDisabled
    }

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .opacity(
                !isEnabled && dimsWhenDisabled
                    ? ThemeOpacity.off : configuration.isPressed ? 0.5 : 1
            )
    }
}

extension PrimitiveButtonStyle
where Self == AccentFocusButtonStyle<PlainButtonLook> {
    /// `.plain` with bae's off opacity, wearing the accent's focus ring.
    public static var accentPlain: Self {
        PlainButtonLook().accentFocus()
    }

    /// `accentPlain` that draws a disabled button undimmed when
    /// `dimsWhenDisabled` is false.
    public static func accentPlain(dimsWhenDisabled: Bool) -> Self {
        PlainButtonLook(dimsWhenDisabled: dimsWhenDisabled).accentFocus()
    }
}

extension PrimitiveButtonStyle
where Self == AccentFocusBorderlessButtonStyle {
    /// `.borderless`, wearing the accent's focus ring.
    public static var accentBorderless: Self {
        .init()
    }
}

extension ButtonStyle {
    /// This style wearing the accent's focus ring.
    public func accentFocus(
        cornerRadius: CGFloat = ThemeRadius.control
    ) -> AccentFocusButtonStyle<Self> {
        AccentFocusButtonStyle(base: self, cornerRadius: cornerRadius)
    }
}
