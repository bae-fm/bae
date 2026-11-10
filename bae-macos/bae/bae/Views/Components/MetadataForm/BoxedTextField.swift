import BaeKit
import SwiftUI

extension View {
    /// This text field in the shared boxed chrome, for a field that doesn't
    /// keep its own focus state: the field's height, inset, corners, fill and
    /// focus ring are `FieldChrome`'s.
    func boxedTextField() -> some View {
        modifier(BoxedTextField())
    }
}

private struct BoxedTextField: ViewModifier {
    @FocusState
    private var focused: Bool

    func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .themeText(.body)
            .focused($focused)
            .modifier(FieldChrome(focused: focused, style: .boxed))
    }
}
