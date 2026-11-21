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
                let secondary = NSColor(Theme.macTextSecondary)
                var mixed = secondary
                appearance.performAsCurrentDrawingAppearance {
                    mixed =
                        secondary.blended(
                            withFraction: 0.5,
                            of: NSColor(Theme.textTertiary)
                        ) ?? secondary
                }
                return mixed
            }
        )
    }
#endif
