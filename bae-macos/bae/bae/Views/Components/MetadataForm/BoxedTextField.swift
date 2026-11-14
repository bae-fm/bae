import BaeKit
import SwiftUI

extension View {
    /// This text field in the shared boxed chrome, for a field that doesn't
    /// keep its own focus state: the field's height, inset, corners, fill and
    /// focus ring are `FieldChrome`'s. `multiline` is for a field of several
    /// lines; `focusesOnAppear` takes the focus as the field appears.
    func boxedTextField(
        multiline: Bool = false,
        focusesOnAppear: Bool = false
    ) -> some View {
        modifier(
            BoxedTextField(
                multiline: multiline,
                focusesOnAppear: focusesOnAppear
            )
        )
    }
}

private struct BoxedTextField: ViewModifier {
    let multiline: Bool
    let focusesOnAppear: Bool
    @FocusState
    private var focused: Bool

    func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .themeText(.body)
            .focused($focused)
            .task {
                if focusesOnAppear { focused = true }
            }
            .modifier(
                FieldChrome(
                    focused: focused,
                    style: multiline ? .boxedMultiline : .boxed
                )
            )
    }
}
