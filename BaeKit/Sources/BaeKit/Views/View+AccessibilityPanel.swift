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
}
