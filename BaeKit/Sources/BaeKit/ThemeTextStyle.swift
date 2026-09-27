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
            case .medium: .medium
            case .semibold: .semibold
            case .bold: .bold
            case .heavy: .heavy
            default: .regular
            }
        }
    }
#endif
