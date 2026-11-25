import BaeKit
import SwiftUI

/// The hover fill of a button that is only its words: it reaches past the
/// words without moving them.
struct HoverPlate: View {
    let hovering: Bool
    let cornerRadius: CGFloat

    var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius)
            .fill(hovering ? Theme.hover : Color.clear)
            .padding(.horizontal, -ThemeSpace.compact)
            .padding(.vertical, -ThemeSpace.line)
    }
}

extension View {
    /// A button that is only its words, with its `HoverPlate`: the button lays
    /// out as its words.
    func hoverPlate(
        _ hovering: Bool,
        cornerRadius: CGFloat = ThemeRadius.control
    ) -> some View {
        background {
            HoverPlate(hovering: hovering, cornerRadius: cornerRadius)
        }
    }
}
