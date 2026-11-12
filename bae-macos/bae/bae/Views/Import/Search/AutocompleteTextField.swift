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

/// A bezeled text field that wears the board's focus ring in the accent the
/// person chose while it is being edited: AppKit's own ring is drawn in the
/// Mac's accent.
private final class RingedTextField: FocusingTextField {
    var ringColor: NSColor {
        get { ring.color }
        set {
            ring.color = newValue
            ring.update(in: self)
        }
    }

    private let ring = AccentFocusRingLayer(cornerRadius: ThemeRadius.chip)

    override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        if became {
            ring.shown = true
            ring.update(in: self)
        }
        return became
    }

    override func textDidEndEditing(_ notification: Notification) {
        super.textDidEndEditing(notification)
        ring.shown = false
        ring.update(in: self)
    }

    override func layout() {
        super.layout()
        ring.update(in: self)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        ring.update(in: self)
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

    @Environment(\.accentChoice)
    private var accent

    func makeNSView(context: Context) -> NSTextField {
        let field = RingedTextField()
        field.ringColor = accent.appKitColor
        field.focus = focus
        field.placeholderString = placeholder
        field.bezelStyle = .roundedBezel
        field.isBordered = true
        field.isBezeled = true
        field.drawsBackground = false
        field.focusRingType = .none
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
        (field as? RingedTextField)?.ringColor = accent.appKitColor
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

            let extended =
                currentText.hasPrefixIgnoringCase(userTypedPrefix)
                && currentText.count > userTypedPrefix.count

            if !extended {
                userTypedPrefix = currentText
                return
            }

            userTypedPrefix = currentText

            let match = parent.suggestions.first {
                $0.count > currentText.count
                    && $0.hasPrefixIgnoringCase(currentText)
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

extension String {
    /// Whether the string starts with `prefix`, ignoring case by the rules of
    /// `locale` (in Turkish "I" matches "ı" and "İ" matches "i"). An empty
    /// prefix is a prefix of everything.
    func hasPrefixIgnoringCase(
        _ prefix: String,
        locale: Locale = .current
    ) -> Bool {
        prefix.isEmpty
            || range(
                of: prefix,
                options: [.caseInsensitive, .anchored],
                locale: locale
            ) != nil
    }
}
