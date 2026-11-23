import BaeKit
import SwiftUI

/// A text field drawn as its value, with the field's fill and edge appearing
/// while the pointer is over it or it has focus; it commits like
/// `CommittedTextField`.
struct MacInlineField: View {
    let placeholder: String
    /// The stored value, which the field shows while it is not focused.
    let value: String
    /// What assistive technology calls the field.
    var name: String?
    var monospaced = false
    /// The field is as wide as its value, and `emptyWidth` while it is empty,
    /// so there is something to click into: for a field in a list row. Any
    /// other field fills the width its column gives it, as the board's does.
    var fitsValue = false
    var font: NSFont = ThemeText.body.nsFont
    var textColor = NSColor(Theme.textPrimary)
    var placeholderRole: CommittedTextField.PlaceholderRole = .hint
    var editingCommands: EditingCommitCommands?
    /// Send the typed value to wherever it lives.
    let onCommit: @MainActor (String) async -> Void

    static let emptyWidth: CGFloat = 96

    var body: some View {
        // The field pulls out by its padding so its words keep the column's
        // edge: on the leading side for a field as wide as its value, and on
        // both for one that fills its column, which the board draws as the
        // column's width plus both paddings.
        Group {
            if fitsValue {
                field
                    .fixedSize(horizontal: true, vertical: false)
                    .padding(.leading, -FieldChrome.inlineHorizontalPadding)
            }
            else {
                field
                    .padding(.horizontal, -FieldChrome.inlineHorizontalPadding)
            }
        }
    }

    private var field: some View {
        CommittedTextField(
            placeholder: placeholder,
            value: value,
            name: name,
            monospaced: monospaced,
            chrome: .inline,
            fillsWidth: !fitsValue,
            emptyMinWidth: fitsValue ? Self.emptyWidth : nil,
            font: font,
            textColor: textColor,
            placeholderRole: placeholderRole,
            editingCommands: editingCommands,
            onCommit: onCommit
        )
    }
}
