import SwiftUI

#if os(macOS)
    import AppKit
#endif

extension AccentChoice {
    public func color(in scheme: ColorScheme) -> Color {
        scheme == .dark ? colors.dark : colors.light
    }

    /// The accent's swatch colour, which a chosen checkbox is filled with.
    public var fillColor: Color {
        colors.fill
    }

    /// The fill behind white button labels.
    public var buttonColor: Color {
        colors.selection
    }
}

#if os(macOS)
    extension AccentChoice {
        /// The accent for AppKit to draw with, resolved in the light or dark
        /// appearance of whatever draws it.
        public var appKitColor: NSColor {
            NSColor(name: nil) { appearance in
                let dark =
                    appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                return NSColor(color(in: dark ? .dark : .light))
            }
        }
    }
#endif
