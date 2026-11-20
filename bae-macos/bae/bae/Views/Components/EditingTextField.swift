import AppKit

/// A text field that knows whether the person is typing in it: whether its
/// field editor is the window's first responder.
@MainActor
protocol EditingField: NSTextField {}

extension EditingField {
    /// Whether the field's editor is the window's first responder.
    var isEditing: Bool {
        guard let editor = currentEditor() else { return false }
        return window?.firstResponder === editor
    }
}

/// A text field that says when it gains and loses its field editor, which is
/// what "focused" is for a field a person types into.
///
/// A secure field (`NSSecureTextField`) says the same with overrides of its
/// own: its class is AppKit's, and Swift adds overrides to a class only
/// through a subclass of it.
class EditingTextField: FocusingTextField, EditingField {
    /// Called with `true` when the field takes its editor and `false` when
    /// the editing ends.
    var onEditingChange: ((Bool) -> Void)?

    override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        if became { onEditingChange?(true) }
        return became
    }

    override func textDidEndEditing(_ notification: Notification) {
        super.textDidEndEditing(notification)
        onEditingChange?(false)
    }
}
