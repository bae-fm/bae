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
                var mixed = NSColor.secondaryLabelColor
                appearance.performAsCurrentDrawingAppearance {
                    mixed =
                        NSColor.secondaryLabelColor.blended(
                            withFraction: 0.5,
                            of: .tertiaryLabelColor
                        ) ?? .secondaryLabelColor
                }
                return mixed
            }
        )
    }
#endif
