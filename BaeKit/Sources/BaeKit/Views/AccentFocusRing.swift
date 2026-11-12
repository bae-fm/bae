import SwiftUI

/// The ring a keyboard-focused control wears, in the accent the person chose.
/// macOS draws its own ring in the Mac's accent, which bae never shows, so a
/// control turns its own off (`focusEffectDisabled`) and wears this one: the
/// board's, three points outside the shape, the accent at 45 percent.
private struct AccentFocusRing: ViewModifier {
    let focused: Bool
    let cornerRadius: CGFloat

    private static let width: CGFloat = 3
    private static let opacity = 0.45

    func body(content: Content) -> some View {
        content.overlay {
            if focused {
                RoundedRectangle(
                    cornerRadius: cornerRadius + Self.width / 2
                )
                .stroke(
                    Theme.accent.opacity(Self.opacity),
                    lineWidth: Self.width
                )
                .padding(-Self.width / 2)
                .allowsHitTesting(false)
            }
        }
    }
}

/// Reads the focus of the control it is put on and wears the ring while that
/// control has it.
private struct FocusedAccentRing: ViewModifier {
    let cornerRadius: CGFloat

    @FocusState
    private var focused: Bool

    func body(content: Content) -> some View {
        content
            .focused($focused)
            .focusEffectDisabled()
            .modifier(
                AccentFocusRing(focused: focused, cornerRadius: cornerRadius)
            )
    }
}

extension View {
    /// This control with its own focus ring replaced by the accent's: the
    /// ring shows while the control has the keyboard focus and follows a
    /// rounded shape of `cornerRadius`.
    public func accentFocusRing(cornerRadius: CGFloat) -> some View {
        modifier(FocusedAccentRing(cornerRadius: cornerRadius))
    }

    /// The ring for a control that already tracks its own focus, such as a
    /// field with a `FocusState`, or a part of a control that has it.
    public func accentFocusRing(
        _ focused: Bool,
        cornerRadius: CGFloat
    ) -> some View {
        modifier(AccentFocusRing(focused: focused, cornerRadius: cornerRadius))
    }
}
