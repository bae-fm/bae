import BaeKit
import SwiftUI

/// The shared field chrome: recessed fill + hairline border that becomes a
/// lifted fill + accent border on focus. `boxed` controls the resting look;
/// the focused look is identical for boxed and borderless cells.
struct FieldChrome: ViewModifier {
    let focused: Bool
    let boxed: Bool

    func body(content: Content) -> some View {
        content
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(restingFill)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay {
                RoundedRectangle(cornerRadius: 6)
                    .strokeBorder(borderColor, lineWidth: focused ? 1.5 : 1)
            }
    }

    private var restingFill: Color {
        if focused {
            return Theme.fieldHover
        }
        return boxed ? Theme.field : .clear
    }

    private var borderColor: Color {
        if focused {
            return Theme.accent
        }
        return boxed ? .white.opacity(0.07) : .clear
    }
}
