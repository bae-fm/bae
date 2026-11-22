#if os(macOS)
    import AppKit
    import SwiftUI

    extension View {
        /// Sets text as the note under a text field: halfway between the
        /// design system's `text-secondary` and `text-tertiary`, so it doesn't
        /// compete with the field's own text.
        public func fieldNote() -> some View {
            themeText(.detail)
                .foregroundStyle(FieldNote.color)
        }
    }

    private enum FieldNote {
        static let color = Color(
            nsColor: NSColor(name: nil) { appearance in
                let secondary = NSColor(Theme.textSecondary)
                var mixed = secondary
                appearance.performAsCurrentDrawingAppearance {
                    guard
                        let blend = secondary.blended(
                            withFraction: 0.5,
                            of: NSColor(Theme.textTertiary)
                        )
                    else {
                        preconditionFailure(
                            "The text colours share no colour space to mix in"
                        )
                    }
                    mixed = blend
                }
                return mixed
            }
        )
    }
#endif
