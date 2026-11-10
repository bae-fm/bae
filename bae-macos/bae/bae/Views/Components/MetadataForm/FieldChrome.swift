import BaeKit
import SwiftUI

/// The shared text-field chrome, `.boxed` for grouped cards and `.inline` for
/// running text or table cells, with the same padding in every state.
struct FieldChrome: ViewModifier {
    enum Style {
        case boxed
        case inline
    }

    let focused: Bool
    let style: Style

    /// How far an inline field's text sits inside its chrome.
    static let inlineHorizontalPadding = ThemeSpace.compact
    static let inlineVerticalPadding = ThemeSpace.line

    @State
    private var hovering = false
    /// Whether this field pushed the I-beam cursor and owes it a pop.
    @State
    private var cursorPushed = false

    func body(content: Content) -> some View {
        content
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .frame(minHeight: style == .boxed ? ThemeSize.field : nil)
            // Round the fill, not a clip: a clip cuts the text off when AppKit
            // remounts the field with a too-short height.
            .background(fill, in: RoundedRectangle(cornerRadius: cornerRadius))
            .overlay {
                RoundedRectangle(cornerRadius: cornerRadius)
                    .strokeBorder(ring, lineWidth: ringWidth)
            }
            .onHover { hovering = $0 }
            .onChange(of: hovering && style == .inline) { _, wantsIBeam in
                // The I-beam covers the inline field's padding too.
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
        case .boxed: ThemeSpace.fieldInset
        case .inline: Self.inlineHorizontalPadding
        }
    }

    private var verticalPadding: CGFloat {
        switch style {
        case .boxed: 0
        case .inline: Self.inlineVerticalPadding
        }
    }

    private var cornerRadius: CGFloat {
        switch style {
        case .boxed: ThemeRadius.field
        case .inline: ThemeRadius.chip
        }
    }

    private var fill: AnyShapeStyle {
        switch style {
        case .boxed:
            AnyShapeStyle(focused ? Theme.fieldHover : Theme.field)
        case .inline:
            if focused {
                AnyShapeStyle(Theme.hover)
            }
            else if hovering {
                AnyShapeStyle(Theme.hover)
            }
            else {
                AnyShapeStyle(Color.clear)
            }
        }
    }

    /// The field's edge: the accent while focused; at rest none in Dark, where
    /// the fill already sets a boxed field apart, and a soft line in Light.
    private var ring: Color {
        switch style {
        case .boxed:
            focused ? Theme.accent : Theme.fieldEdge
        case .inline:
            focused ? Theme.accent : .clear
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
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text(verbatim: "Boxed · resting")
                .modifier(FieldChrome(focused: false, style: .boxed))
            Text(verbatim: "Boxed · focused")
                .modifier(FieldChrome(focused: true, style: .boxed))
            Text(verbatim: "Inline · resting")
                .modifier(FieldChrome(focused: false, style: .inline))
            Text(verbatim: "Inline · focused")
                .modifier(FieldChrome(focused: true, style: .inline))
        }
        .themeText(.body)
        .padding(ThemeSpace.section)
        .frame(width: 300, alignment: .leading)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
