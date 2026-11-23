import SwiftUI

extension View {
    /// Lets the person select this text, the code it shows, which reads to
    /// VoiceOver as one text: a selectable `Text` draws a second text of the
    /// same words inside its own.
    public func selectableCode(_ code: String) -> some View {
        textSelection(.enabled)
            .accessibilityRepresentation { Text(verbatim: code) }
    }
}
