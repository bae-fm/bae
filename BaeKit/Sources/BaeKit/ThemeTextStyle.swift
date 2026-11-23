import SwiftUI

extension View {
    /// Sets text in a theme text role: its font, letter spacing and, on
    /// macOS, the line height the role gives it.
    public func themeText(_ text: ThemeText) -> some View {
        font(text.font)
            .tracking(text.tracking)
            .modifier(ThemeTextLine(text: text))
    }
}

/// Gives text the line height of its role where the role names one: each
/// line is that tall, and so is a single line. A role that names none keeps
/// its font's own line height.
private struct ThemeTextLine: ViewModifier {
    let text: ThemeText

    func body(content: Content) -> some View {
        #if os(macOS)
            if let line = text.macOSLine {
                // SwiftUI puts the spacing between lines only, so the last
                // line gets it below, and n lines are n times the line high.
                let spacing = max(0, line - text.swiftUILineHeight)
                content
                    .lineSpacing(spacing)
                    .padding(.bottom, spacing)
                    .frame(minHeight: line)
            }
            else {
                content
            }
        #else
            content
        #endif
    }
}

extension View {
    /// Sizes an SF Symbol to a theme icon role.
    public func themeIcon(_ icon: ThemeIcon) -> some View {
        font(icon.font)
    }
}

#if os(macOS)
    import AppKit

    extension ThemeText {
        /// The role as an AppKit font, for AppKit text fields and drawing.
        public var nsFont: NSFont {
            monospaced
                ? .monospacedSystemFont(ofSize: macOSSize, weight: nsWeight)
                : .systemFont(ofSize: macOSSize, weight: nsWeight)
        }

        public var nsWeight: NSFont.Weight {
            switch weight {
            case .thin: .thin
            case .medium: .medium
            case .semibold: .semibold
            case .bold: .bold
            case .heavy: .heavy
            default: .regular
            }
        }
    }

    extension ThemeText {
        /// The height SwiftUI lays out for one line of the role's font with
        /// no line height set. It is taller than `NSLayoutManager`'s: at 11
        /// and 11.5 points SwiftUI's line is 14 and AppKit's is 13.
        @MainActor
        var swiftUILineHeight: CGFloat {
            if let known = LineHeights.measured[font] { return known }
            let height = NSHostingView(
                rootView: Text(verbatim: "Ag").font(font)
            )
            .fittingSize.height
            LineHeights.measured[font] = height
            return height
        }
    }

    @MainActor
    private enum LineHeights {
        static var measured: [Font: CGFloat] = [:]
    }

    extension NSFont {
        /// The height of one line of this font with no line height set.
        public var naturalLineHeight: CGFloat {
            NSLayoutManager().defaultLineHeight(for: self)
        }
    }
#endif
