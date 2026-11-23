import BaeKit
import SwiftUI

/// A text field in the shared boxed chrome: the field's height, inset,
/// corners, fill and focus ring are `FieldChrome`'s, and its placeholder is
/// the tertiary colour, which a plain `TextField` on macOS would draw in the
/// system's own. A field under a visible label has no placeholder, since the
/// label names it; its label names it to assistive technology.
/// `multiline` is for a field of several lines; `invalid` turns its edge red.
struct BoxedField<Focus: Hashable>: View {
    /// Who owns the field's focus: the field itself, or a caller that moves
    /// the focus between its fields. Either way one value says whether the
    /// field is focused, and the ring draws from it.
    enum FocusOwner {
        /// The field takes the focus as it appears when `onAppear`.
        case own(onAppear: Bool)
        /// The field is focused when the caller's `FocusState` holds `value`.
        case shared(FocusState<Focus>.Binding, value: Focus)
    }

    let placeholder: LocalizedStringKey?
    @Binding
    var text: String
    let font: ThemeText
    let secret: Bool
    let multiline: Bool
    let invalid: Bool
    let owner: FocusOwner
    @FocusState
    private var ownFocus: Bool

    /// A multiline field is at least this many lines of its text tall, as
    /// the boards draw a code field.
    private static var multilineMinHeight: CGFloat { 3 * 20 }

    var body: some View {
        let chrome = FieldChrome(
            focused: isFocused,
            style: multiline ? .boxedMultiline : .boxed,
            invalid: invalid
        )
        let box =
            focusedField
            .frame(
                minHeight: multiline ? Self.multilineMinHeight : nil,
                alignment: .topLeading
            )
            .modifier(chrome)
        if let placeholder {
            box.accessibilityLabel(Text(placeholder))
        }
        else {
            box
        }
    }

    private var isFocused: Bool {
        switch owner {
        case .own: ownFocus
        case .shared(let binding, let value): binding.wrappedValue == value
        }
    }

    @ViewBuilder
    private var focusedField: some View {
        switch owner {
        case .own(let onAppear):
            textField
                .focused($ownFocus)
                .task {
                    if onAppear { ownFocus = true }
                }
        case .shared(let binding, let value):
            textField.focused(binding, equals: value)
        }
    }

    @ViewBuilder
    private var textField: some View {
        field
            .labelsHidden()
            .textFieldStyle(.plain)
            .themeText(font)
    }

    @ViewBuilder
    private var field: some View {
        if secret {
            SecureField("", text: $text, prompt: prompt)
        }
        else if multiline {
            TextField("", text: $text, prompt: prompt, axis: .vertical)
                .lineLimit(3...)
        }
        else {
            TextField("", text: $text, prompt: prompt)
        }
    }

    /// The placeholder, which assistive technology reads as the field's
    /// placeholder, in the tertiary colour.
    private var prompt: Text? {
        placeholder.map { Text($0).foregroundColor(Theme.textTertiary) }
    }

    /// A field that owns its focus.
    init(
        placeholder: LocalizedStringKey? = nil,
        text: Binding<String>,
        font: ThemeText = .body,
        secret: Bool = false,
        multiline: Bool = false,
        invalid: Bool = false,
        focusesOnAppear: Bool = false
    ) where Focus == Bool {
        self.placeholder = placeholder
        _text = text
        self.font = font
        self.secret = secret
        self.multiline = multiline
        self.invalid = invalid
        owner = .own(onAppear: focusesOnAppear)
    }

    /// A field whose focus the caller owns: it is focused when `focus` holds
    /// `value`.
    init(
        placeholder: LocalizedStringKey? = nil,
        text: Binding<String>,
        focus: FocusState<Focus>.Binding,
        equals value: Focus,
        secret: Bool = false,
        invalid: Bool = false
    ) {
        self.placeholder = placeholder
        _text = text
        font = .body
        self.secret = secret
        multiline = false
        self.invalid = invalid
        owner = .shared(focus, value: value)
    }
}
