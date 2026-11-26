import SwiftUI

extension View {
    /// The box VoiceOver and the shots give this view: its frame, reaching
    /// each given number of points past that side (negative, short of it).
    /// A board boxes a button by its fill, which can reach past the layout.
    /// Put it outside the button style: a style that sets its own box
    /// (`accentPlain`) overrides one set inside it.
    func accessibilityBox(
        leading: CGFloat = 0,
        trailing: CGFloat = 0,
        top: CGFloat = 0,
        bottom: CGFloat = 0
    ) -> some View {
        contentShape(
            .accessibility,
            ReachedRectangle(
                leading: leading,
                trailing: trailing,
                top: top,
                bottom: bottom
            )
        )
    }
}

private struct ReachedRectangle: Shape {
    let leading: CGFloat
    let trailing: CGFloat
    let top: CGFloat
    let bottom: CGFloat

    func path(in rect: CGRect) -> Path {
        Path(
            CGRect(
                x: rect.minX - leading,
                y: rect.minY - top,
                width: rect.width + leading + trailing,
                height: rect.height + top + bottom
            )
        )
    }
}
