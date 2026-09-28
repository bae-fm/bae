import SwiftUI

extension View {
    /// Sets text in a theme text role: its font, letter spacing and case.
    public func themeText(_ text: ThemeText) -> some View {
        font(text.font)
            .tracking(text.tracking)
            .textCase(text.uppercase ? .uppercase : nil)
    }
}

/// The small capitalised label that names a section, a group or a column.
public struct Eyebrow: View {
    private let text: Text

    public init(_ text: Text) {
        self.text = text
    }

    public init(_ key: LocalizedStringKey) {
        text = Text(key)
    }

    public init(verbatim string: String) {
        text = Text(verbatim: string)
    }

    public var body: some View {
        text
            .themeText(.eyebrow)
            .foregroundStyle(.secondary)
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

#if os(iOS)
    import UIKit

    extension ThemeText {
        /// The role as a UIKit font at the person's text size.
        public var uiFont: UIFont {
            let size = UIFont.preferredFont(forTextStyle: uiTextStyle).pointSize
            return monospaced
                ? .monospacedSystemFont(ofSize: size, weight: uiWeight)
                : .systemFont(ofSize: size, weight: uiWeight)
        }

        public var uiWeight: UIFont.Weight {
            switch weight {
            case .thin: .thin
            case .medium: .medium
            case .semibold: .semibold
            case .bold: .bold
            case .heavy: .heavy
            default: .regular
            }
        }

        private var uiTextStyle: UIFont.TextStyle {
            switch iOSStyle {
            case .largeTitle: .largeTitle
            case .title: .title1
            case .title2: .title2
            case .title3: .title3
            case .headline: .headline
            case .subheadline: .subheadline
            case .callout: .callout
            case .footnote: .footnote
            case .caption: .caption1
            case .caption2: .caption2
            default: .body
            }
        }
    }
#endif
