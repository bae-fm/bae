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
    var fillsWidth = false
    var font: NSFont = ThemeText.body.nsFont
    var textColor = NSColor(Theme.textPrimary)
    var placeholderRole: CommittedTextField.PlaceholderRole = .hint
    var editingCommands: EditingCommitCommands?
    /// Send the typed value to wherever it lives.
    let onCommit: @MainActor (String) async -> Void

    var body: some View {
        CommittedTextField(
            placeholder: placeholder,
            value: value,
            name: name,
            monospaced: monospaced,
            chrome: .inline,
            fillsWidth: fillsWidth,
            font: font,
            textColor: textColor,
            placeholderRole: placeholderRole,
            editingCommands: editingCommands,
            onCommit: onCommit
        )
    }
}
