import BaeKit
import SwiftUI

/// The shared text-field chrome, `.boxed` for grouped cards (`.boxedMultiline`
/// for one that holds several lines) and `.inline` for running text or table
/// cells, with the same padding in every state.
struct FieldChrome: ViewModifier {
    enum Style {
        case boxed
        /// The boxed well for text that runs over several lines: it grows
        /// with its text, which sits `ThemeSpace.related` below its top.
        case boxedMultiline
        case inline
    }

    let focused: Bool
    let style: Style
    /// The field holds something that cannot be kept: its edge turns red.
    var invalid = false

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
            // Four unblurred shadows, each cast by the one before it, so the
            // glow is the solid ring the boards draw, and takes no part in the
            // field's layout or accessibility frame.
            .shadow(color: halo ?? .clear, radius: 0, x: Self.haloWidth, y: 0)
            .shadow(color: halo ?? .clear, radius: 0, x: -Self.haloWidth, y: 0)
            .shadow(color: halo ?? .clear, radius: 0, x: 0, y: Self.haloWidth)
            .shadow(color: halo ?? .clear, radius: 0, x: 0, y: -Self.haloWidth)
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

    /// How far a style's text sits inside its chrome, on each side.
    static func padding(of style: Style) -> NSSize {
        switch style {
        case .boxed: NSSize(width: ThemeSpace.fieldInset, height: 0)
        case .boxedMultiline:
            NSSize(width: ThemeSpace.fieldInset, height: ThemeSpace.related)
        case .inline:
            NSSize(
                width: inlineHorizontalPadding,
                height: inlineVerticalPadding
            )
        }
    }

    private var horizontalPadding: CGFloat { Self.padding(of: style).width }

    private var verticalPadding: CGFloat { Self.padding(of: style).height }

    private var cornerRadius: CGFloat {
        switch style {
        case .boxed, .boxedMultiline: ThemeRadius.field
        case .inline: ThemeRadius.chip
        }
    }

    private var fill: AnyShapeStyle {
        switch style {
        case .boxed, .boxedMultiline:
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

    /// The field's edge: red while it holds something that cannot be kept;
    /// otherwise, for an inline field, the accent while focused; at rest none
    /// in Dark, where the fill already sets a boxed field apart, and a soft
    /// line in Light.
    private var ring: Color {
        switch style {
        case .boxed, .boxedMultiline:
            invalid ? Theme.danger : Theme.fieldEdge
        case .inline:
            focused ? Theme.accent : .clear
        }
    }

    private var ringWidth: CGFloat { 1 }

    /// The width of the glow a boxed field draws outside its edge.
    private static let haloWidth: CGFloat = 3

    /// A boxed field's glow outside its edge: red at 35% while it holds
    /// something that cannot be kept, else the accent at 45% while focused.
    private var halo: Color? {
        switch style {
        case .boxed, .boxedMultiline:
            if invalid { return Theme.danger.opacity(0.35) }
            return focused ? Theme.accent.opacity(0.45) : nil
        case .inline:
            return nil
        }
    }
}
