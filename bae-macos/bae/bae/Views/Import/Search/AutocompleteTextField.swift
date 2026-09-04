import AppKit
import SwiftUI

/// Plain text field with inline prefix completion — no dropdown.
/// Typing extends the typed prefix; the first case-insensitive
/// prefix-matching suggestion is inserted after the cursor with the
/// auto-filled tail selected. Tab / Enter / losing focus commits the
/// field value as-is (the selection just deselects — the text is
/// already there). Deleting never re-triggers completion.
///
/// A small progress spinner overlays the trailing edge while
/// `isLoading` is true.
struct AutocompleteTextField: View {
    @Binding
    var text: String
    let placeholder: String
    let suggestions: [String]
    let isLoading: Bool
    /// A request for the field to take the keyboard. Each new request (any
    /// value the field has not served yet; `0` is no request) moves the
    /// responder once, so the same request asked again after the person
    /// clicked elsewhere is honoured while a redraw with an old one is not.
    var focusRequest: Int = 0
    var onSubmit: (() -> Void)?

    var body: some View {
        ZStack(alignment: .trailing) {
            InlineCompletionTextFieldNS(
                text: $text,
                placeholder: placeholder,
                suggestions: suggestions,
                focusRequest: focusRequest,
                onSubmit: onSubmit,
            )
            if isLoading {
                ProgressView()
                    .controlSize(.small)
                    .padding(.trailing, 6)
                    .allowsHitTesting(false)
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
    let focusRequest: Int
    var onSubmit: (() -> Void)?

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
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
        // A request moves the responder once: holding the same one through
        // redraws must not drag focus back every time the pane redraws.
        if focusRequest != 0,
            focusRequest != context.coordinator.servedFocusRequest
        {
            context.coordinator.servedFocusRequest = focusRequest
            // The field has no window during the update pass that installs
            // it, so ask on the next turn.
            DispatchQueue.main.async {
                field.window?.makeFirstResponder(field)
            }
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
        /// The focus request already honoured, so focus moves on a new one
        /// rather than on every redraw.
        fileprivate var servedFocusRequest = 0

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
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Idle") {
        AutocompleteTextField(
            text: .constant(""),
            placeholder: "Artist",
            suggestions: ["Artist Name", "Album Title", "Label Name"],
            isLoading: false,
            onSubmit: {},
        )
        .frame(width: 260)
        .padding()
        .windowBackground()
    }

    #Preview("Loading with text") {
        AutocompleteTextField(
            text: .constant("Art"),
            placeholder: "Artist",
            suggestions: ["Artist Name"],
            isLoading: true,
            onSubmit: {},
        )
        .frame(width: 260)
        .padding()
        .windowBackground()
    }
#endif
