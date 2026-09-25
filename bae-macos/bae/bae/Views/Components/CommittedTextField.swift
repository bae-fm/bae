import AppKit
import BaeKit
import Combine
import SwiftUI

/// One request to finish every field edit currently active in a view tree.
/// Subscribers add the write represented by their focused draft; the sender
/// waits for those writes before it replaces the values underneath them.
@MainActor
final class EditingCommitRequest {
    private var writes: [@MainActor () async -> Void] = []

    func append(_ write: @escaping @MainActor () async -> Void) {
        writes.append(write)
    }

    func perform() async {
        for write in writes {
            await write()
        }
    }
}

/// Publishes the one-shot command that commits and unfocuses active fields.
/// The request carries acknowledgements, so replacing metadata waits for the
/// field writes rather than relying on SwiftUI's focus-change delivery order.
@MainActor
final class EditingCommitCommands {
    fileprivate let requests = PassthroughSubject<EditingCommitRequest, Never>()

    func commitActiveEdits() async {
        let request = EditingCommitRequest()
        requests.send(request)
        await request.perform()
    }
}

/// A text field whose value lives somewhere else — a row in the database —
/// and which decides when to send what was typed there.
///
/// One write is one commit, and a commit redraws whatever reads it. That is
/// right per settled value and wrong per keystroke, so the field owns a draft
/// while it has focus and commits it on the three moments a person means "this
/// is the value": leaving the field, pressing Return, and pausing.
///
/// The draft is the only copy anywhere, and it exists only while the field is
/// focused: an unfocused field shows `value`, so a value that changed
/// underneath replaces what is on screen rather than being overwritten by a
/// stale draft.
struct CommittedTextField: View {
    /// What the placeholder is for.
    enum PlaceholderRole {
        /// The system placeholder: says what goes here, and stays while the
        /// empty field is focused.
        case hint
        /// A fact sheet's mark for "nothing here": the tertiary label color
        /// at rest, gone the moment the field is focused.
        case emptyMark
    }

    let placeholder: String
    /// The stored value. Re-seeds the draft whenever the field is not focused.
    let value: String
    var monospaced: Bool = false
    var chrome: FieldChrome.Style = .boxed
    var fillsWidth = false
    var font: NSFont = .systemFont(ofSize: 13)
    /// The typed value's color. The placeholder takes its own, by role.
    var textColor: NSColor = .controlTextColor
    var placeholderRole: PlaceholderRole = .hint
    /// Present on surfaces that can replace the stored value while this field
    /// is focused. Other editors commit through focus, Return, and pause only.
    var editingCommands: EditingCommitCommands?
    /// Send the typed value to wherever it lives.
    let onCommit: @MainActor (String) async -> Void

    /// How long a pause counts as "done typing".
    static let commitDelay: Duration = .milliseconds(400)

    @State
    private var draft: String = ""
    @State
    private var pending: Task<Void, Never>?
    /// Whether the field is being edited: AppKit's answer, reported by the
    /// field as it gains and loses its field editor. Setting it false ends
    /// the editing.
    @State
    private var focused = false
    @State
    private var suppressNextBlurCommit = false

    @ViewBuilder
    var body: some View {
        if let editingCommands {
            configuredField
                .onReceive(editingCommands.requests) { request in
                    guard focused else { return }
                    let text = draft
                    pending?.cancel()
                    pending = nil
                    suppressNextBlurCommit = true
                    focused = false
                    guard text != value else { return }
                    request.append { await onCommit(text) }
                }
        }
        else {
            configuredField
        }
    }

    private var configuredField: some View {
        field
            .modifier(FieldChrome(focused: focused, style: chrome))
            .onAppear { draft = value }
            .onChange(of: value) { _, next in
                // A field being typed into owns what it shows; anything else
                // takes the stored value as it lands.
                if !focused { draft = next }
            }
            .onChange(of: draft) { _, next in
                guard focused else { return }
                pending?.cancel()
                pending = Task {
                    try? await Task.sleep(for: Self.commitDelay)
                    guard !Task.isCancelled else { return }
                    await commit(next)
                }
            }
            .onChange(of: focused) { _, isFocused in
                guard !isFocused else { return }
                if suppressNextBlurCommit {
                    suppressNextBlurCommit = false
                    return
                }
                startCommit(draft)
            }
    }

    /// How far the text field's cell insets its text from each side of the
    /// field. Measured at 3.5–3.8 points across the two sides for every font
    /// the fields use, so two a side leaves nothing of the last glyph outside
    /// the field's bounds.
    private static let cellInset: CGFloat = 2

    /// The field, laid out by a hidden Text of the same font and content and
    /// drawn in exactly the frame that Text takes.
    ///
    /// A SwiftUI `TextField` sizes its `NSTextField` itself, and that size is
    /// not the field's own: a hosting now and then gives an empty 12.5-point
    /// fact field the 26-point height of the album title beside it, or the
    /// title the 16-point height of a track row, while AppKit measures the
    /// field at its own font's height throughout. The field draws its text
    /// placed by that borrowed height — half a point off, or with the top
    /// of a 22-point title cut away. A Text measures from its font every
    /// time, so the Text lays the field out and the editor below fills that
    /// frame and nothing else. The width follows the text unless the caller
    /// asks the editor to fill its column.
    private var field: some View {
        Text(verbatim: draft.isEmpty ? placeholder : draft)
            .font(Font(draft.isEmpty ? font : valueFont))
            .lineLimit(1)
            .padding(.horizontal, Self.cellInset)
            .hidden()
            .frame(maxWidth: fillsWidth ? .infinity : nil, alignment: .leading)
            .overlay {
                CommittedTextEditor(
                    text: $draft,
                    focused: $focused,
                    placeholder: placeholder,
                    placeholderRole: placeholderRole,
                    font: valueFont,
                    placeholderFont: font,
                    textColor: textColor,
                    onSubmit: { startCommit(draft) }
                )
            }
    }

    /// The value takes the monospaced design; the placeholder keeps `font`
    /// as given, so an empty mark is the same glyph in every field.
    private var valueFont: NSFont {
        guard monospaced,
            let descriptor = font.fontDescriptor.withDesign(.monospaced),
            let monospacedFont = NSFont(
                descriptor: descriptor,
                size: font.pointSize
            )
        else { return font }
        return monospacedFont
    }

    /// Send `text` unless it is already what is stored — a focus change over
    /// an untouched field is not an edit.
    private func startCommit(_ text: String) {
        pending?.cancel()
        pending = Task { await commit(text) }
    }

    func commit(_ text: String) async {
        guard text != value else { return }
        await onCommit(text)
    }
}

/// The `NSTextField` a committed field edits in, sized to exactly what its
/// caller proposes: the frame of the Text that lays the field out.
private struct CommittedTextEditor: NSViewRepresentable {
    @Binding
    var text: String
    @Binding
    var focused: Bool
    let placeholder: String
    let placeholderRole: CommittedTextField.PlaceholderRole
    let font: NSFont
    let placeholderFont: NSFont
    let textColor: NSColor
    let onSubmit: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> EditorField {
        let field = EditorField()
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.lineBreakMode = .byClipping
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        field.delegate = context.coordinator
        field.onEditingChange = { [coordinator = context.coordinator] editing in
            if coordinator.parent.focused != editing {
                coordinator.parent.focused = editing
            }
        }
        return field
    }

    func updateNSView(_ field: EditorField, context: Context) {
        context.coordinator.parent = self
        if field.stringValue != text {
            field.stringValue = text
        }
        field.font = font
        field.textColor = textColor
        field.isEnabled = context.environment.isEnabled
        switch placeholderRole {
        case .hint:
            field.placeholderString = placeholder
        case .emptyMark:
            // Gone while editing, so the caret sits alone at the leading edge.
            field.placeholderAttributedString =
                focused
                ? nil
                : NSAttributedString(
                    string: placeholder,
                    attributes: [
                        .font: placeholderFont,
                        .foregroundColor: NSColor.tertiaryLabelColor,
                    ]
                )
        }
        if !focused, field.isEditing {
            field.window?.makeFirstResponder(nil)
        }
    }

    /// Exactly the proposed size: the field fills the frame the Text laid
    /// out, and never reports a height of its own. As an overlay it is
    /// always proposed that frame; it has no other size to offer.
    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView: EditorField,
        context: Context
    ) -> CGSize? {
        proposal.replacingUnspecifiedDimensions(by: .zero)
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: CommittedTextEditor

        init(_ parent: CommittedTextEditor) {
            self.parent = parent
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else {
                return
            }
            parent.text = field.stringValue
        }

        /// Return commits and the field stays in editing. AppKit's own
        /// Return ends the editing and begins it again, which would report
        /// a blur between the two and send the value twice.
        func control(
            _ control: NSControl,
            textView: NSTextView,
            doCommandBy selector: Selector
        ) -> Bool {
            guard selector == #selector(NSResponder.insertNewline(_:)) else {
                return false
            }
            parent.onSubmit()
            return true
        }
    }

    /// A text field that says when it gains and loses its field editor —
    /// which is what "focused" is for a field a person types into.
    final class EditorField: NSTextField {
        var onEditingChange: ((Bool) -> Void)?

        /// Whether the field's editor is the window's first responder.
        var isEditing: Bool {
            guard let editor = currentEditor() else { return false }
            return window?.firstResponder === editor
        }

        override func becomeFirstResponder() -> Bool {
            let became = super.becomeFirstResponder()
            if became {
                onEditingChange?(true)
            }
            return became
        }

        override func textDidEndEditing(_ notification: Notification) {
            super.textDidEndEditing(notification)
            onEditingChange?(false)
        }
    }
}

#if DEBUG
    #Preview("Committed text field") {
        @Previewable
        @State
        var stored = "Album Title"
        VStack(alignment: .leading, spacing: 12) {
            CommittedTextField(
                placeholder: "Album title",
                value: stored,
                onCommit: { stored = $0 },
            )
            Text(verbatim: "Stored: \(stored)")
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
        }
        .padding(24)
        .frame(width: 320)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
