import SwiftUI

/// A system button style with the system's focus ring replaced by the
/// accent's (`accentFocusRing`). The style keeps its own look, keyboard
/// handling, pressed state and VoiceOver role.
public struct AccentFocusPrimitiveButtonStyle<Base: PrimitiveButtonStyle>:
    PrimitiveButtonStyle
{
    let base: Base
    let cornerRadius: CGFloat

    public func makeBody(configuration: Configuration) -> some View {
        Button(configuration)
            .buttonStyle(base)
            .accentFocusRing(cornerRadius: cornerRadius)
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

extension PrimitiveButtonStyle
where Self == AccentFocusPrimitiveButtonStyle<PlainButtonStyle> {
    /// `.plain`, wearing the accent's focus ring.
    public static var accentPlain: Self {
        .init(base: .plain, cornerRadius: ThemeRadius.control)
    }
}

extension PrimitiveButtonStyle
where Self == AccentFocusPrimitiveButtonStyle<BorderlessButtonStyle> {
    /// `.borderless`, wearing the accent's focus ring.
    public static var accentBorderless: Self {
        .init(base: .borderless, cornerRadius: ThemeRadius.control)
    }
}

extension PrimitiveButtonStyle
where Self == AccentFocusPrimitiveButtonStyle<BorderedButtonStyle> {
    /// `.bordered`, wearing the accent's focus ring round its capsule.
    public static var accentBordered: Self {
        .init(base: .bordered, cornerRadius: .infinity)
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
