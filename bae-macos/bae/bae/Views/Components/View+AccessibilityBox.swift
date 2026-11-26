import SwiftUI

extension View {
    /// The box VoiceOver and the shots give this view: its frame, reaching
    /// `leading` and `trailing` points past that side (negative, short of it).
    /// A board boxes a button by its fill, which can reach past the layout.
    /// Put it outside the button style: a style that sets its own box
    /// (`accentPlain`) overrides one set inside it.
    func accessibilityBox(leading: CGFloat, trailing: CGFloat) -> some View {
        contentShape(
            .accessibility,
            ReachedRectangle(leading: leading, trailing: trailing)
        )
    }
}

private struct ReachedRectangle: Shape {
    let leading: CGFloat
    let trailing: CGFloat

    func path(in rect: CGRect) -> Path {
        Path(
            CGRect(
                x: rect.minX - leading,
                y: rect.minY,
                width: rect.width + leading + trailing,
                height: rect.height
            )
        )
    }
}
