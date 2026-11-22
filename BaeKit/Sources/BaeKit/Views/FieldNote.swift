#if os(macOS)
    import AppKit
    import SwiftUI

    extension View {
        /// Sets text as the note under a text field: halfway between
        /// secondary and tertiary text, so it doesn't compete with the
        /// field's own text.
        public func fieldNote() -> some View {
            themeText(.detail)
                .foregroundStyle(FieldNote.color)
        }
    }

    private enum FieldNote {
        static let color = Color(
            nsColor: NSColor(name: nil) { appearance in
                var mixed = NSColor(Theme.macTextSecondary)
                appearance.performAsCurrentDrawingAppearance {
                    mixed = halfway(
                        NSColor(Theme.macTextSecondary),
                        NSColor(Theme.textTertiary)
                    )
                }
                return mixed
            }
        )

        /// The colour halfway between `first` and `second`, mixed with the
        /// alpha counted in, as CSS `color-mix` does: a translucent colour
        /// weighs in by its alpha, so the note matches its board.
        private static func halfway(_ first: NSColor, _ second: NSColor)
            -> NSColor
        {
            guard
                let one = first.usingColorSpace(.sRGB),
                let two = second.usingColorSpace(.sRGB)
            else {
                preconditionFailure(
                    "The text colours share no colour space to mix in"
                )
            }
            let alpha = (one.alphaComponent + two.alphaComponent) / 2
            func channel(_ read: (NSColor) -> CGFloat) -> CGFloat {
                (read(one) * one.alphaComponent + read(two) * two.alphaComponent)
                    / 2 / alpha
            }
            return NSColor(
                srgbRed: channel(\.redComponent),
                green: channel(\.greenComponent),
                blue: channel(\.blueComponent),
                alpha: alpha
            )
        }
    }
#endif
