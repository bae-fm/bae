import BaeKit
import SwiftUI

/// The shared field chrome, in the two shapes a text field takes.
///
/// `.boxed` is the grouped-card field: a recessed well with a hairline border
/// at rest that lifts and gains an accent border on focus. `.inline` is a
/// field set into running text or a table cell: nothing at rest, a faint fill
/// under the pointer, a faint fill and a one-point accent ring while editing.
/// Both keep their padding in every state, so the text never moves and the
/// row never grows as the chrome comes and goes.
struct FieldChrome: ViewModifier {
    enum Style {
        case boxed
        case inline
    }

    let focused: Bool
    let style: Style

    /// How far an inline field's text sits inside its chrome. Surfaces that
    /// line an inline field's text up with plain text beside it offset by
    /// this.
    static let inlineHorizontalPadding: CGFloat = 7
    static let inlineVerticalPadding: CGFloat = 3

    @State
    private var hovering = false
    /// Whether this field pushed the I-beam cursor and owes it a pop.
    @State
    private var cursorPushed = false

    func body(content: Content) -> some View {
        content
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            // The rounding lives on the fill, not on a clip of the content:
            // clipping the field cuts its text off whenever AppKit remounts
            // it with a transiently under-measured height.
            .background(fill, in: RoundedRectangle(cornerRadius: cornerRadius))
            .overlay {
                RoundedRectangle(cornerRadius: cornerRadius)
                    .strokeBorder(ring, lineWidth: ringWidth)
            }
            .onHover { hovering = $0 }
            .onChange(of: hovering && style == .inline) { _, wantsIBeam in
                // The padding around an inline field is part of the field:
                // the pointer says so there, not only over the glyphs.
                if wantsIBeam, !cursorPushed {
                    NSCursor.iBeam.push()
                    cursorPushed = true
                }
                else if !wantsIBeam, cursorPushed {
                    NSCursor.pop()
                    cursorPushed = false
                }
            }
            .onDisappear {
                if cursorPushed {
                    NSCursor.pop()
                    cursorPushed = false
                }
            }
    }

    private var horizontalPadding: CGFloat {
        switch style {
        case .boxed: 10
        case .inline: Self.inlineHorizontalPadding
        }
    }

    private var verticalPadding: CGFloat {
        switch style {
        case .boxed: 6
        case .inline: Self.inlineVerticalPadding
        }
    }

    private var cornerRadius: CGFloat {
        switch style {
        case .boxed: 6
        case .inline: 5
        }
    }

    private var fill: Color {
        switch style {
        case .boxed:
            focused ? Theme.fieldHover : Theme.field
        case .inline:
            if focused {
                .white.opacity(0.06)
            }
            else if hovering {
                .white.opacity(0.05)
            }
            else {
                .clear
            }
        }
    }

    private var ring: Color {
        switch style {
        case .boxed:
            focused ? Theme.accent : .white.opacity(0.07)
        case .inline:
            focused ? Theme.accent.opacity(0.6) : .clear
        }
    }

    private var ringWidth: CGFloat {
        switch style {
        case .boxed: focused ? 1.5 : 1
        case .inline: 1
        }
    }
}

#if DEBUG
    #Preview("Field Chrome") {
        // The resting/focused × boxed/inline combinations the chrome renders,
        // hosting plain text where a real field would sit.
        VStack(alignment: .leading, spacing: 12) {
            Text(verbatim: "Boxed · resting")
                .modifier(FieldChrome(focused: false, style: .boxed))
            Text(verbatim: "Boxed · focused")
                .modifier(FieldChrome(focused: true, style: .boxed))
            Text(verbatim: "Inline · resting")
                .modifier(FieldChrome(focused: false, style: .inline))
            Text(verbatim: "Inline · focused")
                .modifier(FieldChrome(focused: true, style: .inline))
        }
        .font(.system(size: 13))
        .padding(24)
        .frame(width: 300, alignment: .leading)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
