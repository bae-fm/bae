import BaeKit
import SwiftUI

extension View {
    /// The hover fill of a button that is only its words: it reaches past the
    /// words without moving them, so the button lays out as its words.
    func hoverPlate(
        _ hovering: Bool,
        cornerRadius: CGFloat = ThemeRadius.control
    ) -> some View {
        background {
            RoundedRectangle(cornerRadius: cornerRadius)
                .fill(hovering ? Theme.hover : Color.clear)
                .padding(.horizontal, -ThemeSpace.compact)
                .padding(.vertical, -ThemeSpace.line)
        }
    }
}
