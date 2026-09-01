import BaeKit
import SwiftUI

/// The shared field chrome: recessed fill + hairline border that becomes a
/// lifted fill + accent border on focus. `boxed` controls the resting look —
/// a boxed field always shows its well; a borderless one shows nothing at
/// rest and takes the boxed resting look under the pointer, so text that is
/// editable says so the moment it is hovered. The focused look is identical
/// for boxed and borderless cells.
struct FieldChrome: ViewModifier {
    let focused: Bool
    let boxed: Bool

    /// How far the field's text sits inside its chrome. Surfaces that line a
    /// borderless field's text up with plain text beside it, or set rows of
    /// borderless fields at a text-height pitch, offset by these.
    static let horizontalPadding: CGFloat = 10
    static let verticalPadding: CGFloat = 6

    @State
    private var hovering = false

    func body(content: Content) -> some View {
        content
            .padding(.horizontal, Self.horizontalPadding)
            .padding(.vertical, Self.verticalPadding)
            .background(restingFill)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay {
                RoundedRectangle(cornerRadius: 6)
                    .strokeBorder(borderColor, lineWidth: focused ? 1.5 : 1)
            }
            .onHover { hovering = $0 }
    }

    private var restingFill: Color {
        if focused {
            return Theme.fieldHover
        }
        return boxed || hovering ? Theme.field : .clear
    }

    private var borderColor: Color {
        if focused {
            return Theme.accent
        }
        return boxed || hovering ? .white.opacity(0.07) : .clear
    }
}

#if DEBUG
    #Preview("Field Chrome") {
        // The four resting/focused × boxed/borderless combinations the chrome
        // renders, hosting plain text where a real field would sit.
        VStack(alignment: .leading, spacing: 12) {
            Text(verbatim: "Boxed · resting")
                .modifier(FieldChrome(focused: false, boxed: true))
            Text(verbatim: "Boxed · focused")
                .modifier(FieldChrome(focused: true, boxed: true))
            Text(verbatim: "Borderless · resting")
                .modifier(FieldChrome(focused: false, boxed: false))
            Text(verbatim: "Borderless · focused")
                .modifier(FieldChrome(focused: true, boxed: false))
        }
        .font(.system(size: 13))
        .padding(24)
        .frame(width: 300, alignment: .leading)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
