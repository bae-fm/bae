import AppKit
import BaeKit
import Combine
import SwiftUI

/// One request to finish every active field edit in a view tree; the sender
/// waits for the writes subscribers add before replacing the values.
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

/// Commits and unfocuses active fields, waiting for their writes rather than
/// relying on SwiftUI's focus-change order.
@MainActor
final class EditingCommitCommands {
    fileprivate let requests = PassthroughSubject<EditingCommitRequest, Never>()

    func commitActiveEdits() async {
        let request = EditingCommitRequest()
        requests.send(request)
        await request.perform()
    }
}

/// A text field for a value stored elsewhere, whose draft exists only while
/// focused and commits on blur, Return, or a pause in typing.
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
    var font: NSFont = ThemeText.body.nsFont
    /// The typed value's color. The placeholder takes its own, by role.
    var textColor: NSColor = .controlTextColor
    var placeholderRole: PlaceholderRole = .hint
    /// Set on surfaces that can replace the stored value while this field is
    /// focused.
    var editingCommands: EditingCommitCommands?
    /// Send the typed value to wherever it lives.
    let onCommit: @MainActor (String) async -> Void

    /// How long a pause counts as "done typing".
    static let commitDelay: Duration = .milliseconds(400)

    @State
    private var draft: String = ""
    @State
    private var pending: Task<Void, Never>?
    /// Whether AppKit's field editor is in the field; setting it false ends
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

    /// The field cell's text inset on each side; measured at 3.5–3.8 points
    /// across both sides for every font the fields use.
    private static let cellInset: CGFloat = 2

    /// The field, drawn in the frame of a hidden Text of the same font and
    /// content, since a `TextField` can take a neighbour's height and clip.
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

    /// Exactly the proposed size: the frame the Text laid out.
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

        /// Return commits and keeps editing, since AppKit's own Return
        /// reports a blur and would send the value twice.
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
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            CommittedTextField(
                placeholder: "Album title",
                value: stored,
                onCommit: { stored = $0 },
            )
            Text(verbatim: "Stored: \(stored)")
                .themeText(.detail)
                .foregroundStyle(.secondary)
        }
        .padding(ThemeSpace.section)
        .frame(width: 320)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
