import SwiftUI

extension View {
    /// Sets text in a theme text role: its font and letter spacing.
    public func themeText(_ text: ThemeText) -> some View {
        font(text.font)
            .tracking(text.tracking)
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
#endif
