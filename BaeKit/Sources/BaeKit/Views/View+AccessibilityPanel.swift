import SwiftUI

extension View {
    /// Names the view as a group, as the boards' labelled panel is: the
    /// group's box is the whole view, padding included. SwiftUI sizes a
    /// group to the union of its children, which stops at the padding, so a
    /// clear child fills the view.
    public func accessibilityPanel(_ label: Text) -> some View {
        background { Color.clear.accessibilityElement() }
            .accessibilityElement(children: .contain)
            .accessibilityLabel(label)
    }

    /// Lets the person select this text, the code it shows, which reads to
    /// VoiceOver as one text: a selectable `Text` draws a second text of the
    /// same words inside its own.
    public func selectableCode(_ code: String) -> some View {
        textSelection(.enabled)
            .accessibilityRepresentation { Text(verbatim: code) }
    }
}
