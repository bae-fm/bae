import AppKit
import BaeKit
import SwiftUI

/// Plain text field with inline prefix completion — no dropdown.
/// Typing extends the typed prefix; the first case-insensitive
/// prefix-matching suggestion is inserted after the cursor with the
/// auto-filled tail selected. Tab / Enter / losing focus commits the
/// field value as-is (the selection just deselects — the text is
/// already there). Deleting never re-triggers completion.
struct AutocompleteTextField: View {
    @Binding
    var text: String
    let placeholder: String
    let suggestions: [String]
    /// A request for the field to take the keyboard. A pending request
    /// (nonzero) is served when the field shows, and each new value once, so
    /// the same request asked again after the person clicked elsewhere is
    /// honoured while a redraw with an old one is not; `0` is no request.
    var focusRequest: Int = 0
    var onSubmit: (() -> Void)?
    /// The field gave the keyboard up — Tab, a click elsewhere, Enter. What
    /// it holds is what the person left in it.
    var onEditingEnded: (() -> Void)?

    @State
    private var focus = FieldFocus()

    var body: some View {
        InlineCompletionTextFieldNS(
            text: $text,
            placeholder: placeholder,
            suggestions: suggestions,
            focus: focus,
            onSubmit: onSubmit,
            onEditingEnded: onEditingEnded,
        )
        // Served from here rather than from the update pass: taking the
        // keyboard ends another field's editing, whose handler writes state.
        .onChange(of: focusRequest, initial: true) { _, request in
            if request != 0 {
                focus.request()
            }
        }
    }

    nonisolated static func completionSelectionRange(
        currentText: String,
        match: String
    ) -> NSRange {
        let start = (currentText as NSString).length
        let matchLength = (match as NSString).length
        guard start <= matchLength else {
            return NSRange(location: matchLength, length: 0)
        }
        return NSRange(location: start, length: matchLength - start)
    }
}

private struct InlineCompletionTextFieldNS: NSViewRepresentable {
    @Binding
    var text: String
    let placeholder: String
    let suggestions: [String]
    let focus: FieldFocus
    var onSubmit: (() -> Void)?
    var onEditingEnded: (() -> Void)?

    func makeNSView(context: Context) -> NSTextField {
        let field = FocusingTextField()
        field.focus = focus
        field.placeholderString = placeholder
        field.bezelStyle = .roundedBezel
        field.isBordered = true
        field.isBezeled = true
        field.drawsBackground = false
        field.focusRingType = .default
        field.delegate = context.coordinator
        field.target = context.coordinator
        field.action = #selector(Coordinator.submit(_:))
        field.cell?.usesSingleLineMode = true
        field.cell?.wraps = false
        field.cell?.isScrollable = true
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.parent = self
        if field.stringValue != text {
            field.stringValue = text
            context.coordinator.userTypedPrefix = text
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: InlineCompletionTextFieldNS
        /// Tracks what the user has actually typed (vs. what we auto-
        /// filled and selected). The user "extends" the prefix by typing
        /// more characters; typing into the selected auto-filled tail
        /// replaces it and counts as extension too. Deleting or editing
        /// mid-text resets this and suppresses further completion until
        /// the user types forward again.
        fileprivate var userTypedPrefix: String = ""

        init(parent: InlineCompletionTextFieldNS) {
            self.parent = parent
        }

        func controlTextDidChange(_ notification: Notification) {
            guard
                let field = notification.object as? NSTextField,
                let editor = field.currentEditor() as? NSTextView
            else {
                return
            }

            let currentText = field.stringValue
            parent.text = currentText

            let lowerCurrent = currentText.lowercased()
            let lowerPrefix = userTypedPrefix.lowercased()
            let extended =
                lowerCurrent.hasPrefix(lowerPrefix)
                && currentText.count > userTypedPrefix.count

            if !extended {
                userTypedPrefix = currentText
                return
            }

            userTypedPrefix = currentText

            let match = parent.suggestions.first {
                $0.count > currentText.count
                    && $0.lowercased().hasPrefix(lowerCurrent)
            }
            guard let match else {
                return
            }

            field.stringValue = match
            parent.text = match
            editor.selectedRange =
                AutocompleteTextField
                .completionSelectionRange(
                    currentText: currentText,
                    match: match
                )
        }

        @objc
        func submit(_: NSTextField) {
            parent.onSubmit?()
        }

        func controlTextDidEndEditing(_: Notification) {
            parent.onEditingEnded?()
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Suggestions") {
        AutocompleteTextField(
            text: .constant(""),
            placeholder: "Artist",
            suggestions: ["Artist Name", "Album Title", "Label Name"],
            onSubmit: {},
        )
        .frame(width: 260)
        .padding()
        .windowBackground()
    }
#endif
