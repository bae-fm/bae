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
        /// Says what goes here, and stays while the empty field is focused.
        case hint
        /// A fact sheet's mark for "nothing here": gone the moment the field
        /// is focused.
        case emptyMark
    }

    let placeholder: String
    /// The stored value. Re-seeds the draft whenever the field is not focused.
    let value: String
    /// What assistive technology calls the field, such as the row it sits
    /// in; `nil` leaves it to the placeholder.
    var name: String?
    var monospaced: Bool = false
    var chrome: FieldChrome.Style = .boxed
    var fillsWidth = false
    /// The field's least width, chrome included, while it is empty, so there
    /// is something to click into.
    var emptyMinWidth: CGFloat?
    var font: NSFont = ThemeText.body.nsFont
    /// The typed value's color. The placeholder takes its own, by role.
    var textColor = NSColor(Theme.textPrimary)
    var placeholderRole: PlaceholderRole = .hint
    /// The field holds something that cannot be kept: its edge turns red.
    var invalid = false
    /// The typed characters are shown as dots.
    var secret = false
    /// What typing leaves of the text: a field that takes only some
    /// characters drops the rest as they are typed.
    var filter: @MainActor (String) -> String = { $0 }
    /// Set on surfaces that can replace the stored value while this field is
    /// focused.
    var editingCommands: EditingCommitCommands?
    /// Send the typed value to wherever it lives.
    let onCommit: @MainActor (String) async -> Void

    /// How long a pause counts as "done typing".
    static let commitDelay: Duration = .milliseconds(400)

    @Environment(\.debounceClock)
    private var clock

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
            .modifier(
                FieldChrome(focused: focused, style: chrome, invalid: invalid)
            )
            // Over the whole chrome, so the editor's frame is the box the
            // person sees and its text sits inside the chrome's padding.
            .overlay {
                CommittedTextEditor(
                    text: $draft,
                    secret: secret,
                    filter: filter,
                    focused: $focused,
                    placeholder: placeholder,
                    placeholderRole: placeholderRole,
                    name: name,
                    font: valueFont,
                    placeholderFont: font,
                    textColor: textColor,
                    insets: FieldChrome.padding(of: chrome),
                    onSubmit: { startCommit(draft) }
                )
            }
            .onAppear { draft = value }
            .onChange(of: value) { _, next in
                // A field being typed into owns what it shows; anything else
                // takes the stored value as it lands.
                if !focused { draft = next }
            }
            .onChange(of: draft) { _, next in
                guard focused else { return }
                pending?.cancel()
                let clock = clock
                pending = Task {
                    try? await clock.sleep(for: Self.commitDelay)
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

    /// The frame of a hidden Text of the same font and content, which the
    /// chrome pads and the editor fills, since a `TextField` can take a
    /// neighbour's height and clip.
    private var field: some View {
        Text(verbatim: draft.isEmpty ? placeholder : draft)
            .font(Font(draft.isEmpty ? font : valueFont))
            .lineLimit(1)
            .hidden()
            .frame(
                minWidth: draft.isEmpty
                    ? emptyMinWidth.map {
                        $0 - 2 * FieldChrome.padding(of: chrome).width
                    } : nil,
                maxWidth: fillsWidth ? .infinity : nil,
                alignment: .leading
            )
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
    let secret: Bool
    let filter: @MainActor (String) -> String
    @Binding
    var focused: Bool
    let placeholder: String
    let placeholderRole: CommittedTextField.PlaceholderRole
    let name: String?
    let font: NSFont
    let placeholderFont: NSFont
    let textColor: NSColor
    /// How far the text sits inside the field's frame.
    let insets: NSSize
    let onSubmit: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> NSTextField {
        let reportEditing = onEditingChange(context.coordinator)
        let field: NSTextField
        if secret {
            field = SecureEditorField(onEditingChange: reportEditing)
        }
        else {
            let editor = EditingTextField()
            editor.onEditingChange = reportEditing
            field = editor
        }
        let cell: NSTextFieldCell =
            secret
            ? InsetSecureTextFieldCell(textCell: "")
            : InsetTextFieldCell(textCell: "")
        field.useAsBoxedEditor(cell: cell)
        field.delegate = context.coordinator
        return field
    }

    private func onEditingChange(_ coordinator: Coordinator) -> (Bool) -> Void {
        { [coordinator] editing in
            if coordinator.parent.focused != editing {
                coordinator.parent.focused = editing
            }
        }
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.parent = self
        if field.stringValue != text {
            field.stringValue = text
        }
        guard let cell = field.cell as? any InsetCell else {
            preconditionFailure("makeNSView installs an inset cell")
        }
        cell.insets = insets
        field.font = font
        field.textColor = textColor
        field.isEnabled = context.environment.isEnabled
        field.setAccessibilityLabel(name)
        // The placeholder is the tertiary label colour in both roles; an
        // empty mark is gone while editing, so the caret sits alone at the
        // leading edge.
        let showsPlaceholder = placeholderRole == .hint || !focused
        if showsPlaceholder {
            field.setBoxedPlaceholder(placeholder, font: placeholderFont)
        }
        else {
            field.placeholderAttributedString = nil
        }
        if !focused, (field as? any EditingField)?.isEditing == true {
            field.window?.makeFirstResponder(nil)
        }
    }

    /// Exactly the proposed size: the frame the Text laid out.
    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView: NSTextField,
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
            let typed = field.stringValue
            let kept = parent.filter(typed)
            if kept != typed { field.stringValue = kept }
            parent.text = kept
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

    /// A secure field that says what `EditingTextField` says, with the typed
    /// characters shown as dots.
    final class SecureEditorField: NSSecureTextField, EditingField {
        let onEditingChange: (Bool) -> Void

        init(onEditingChange: @escaping (Bool) -> Void) {
            self.onEditingChange = onEditingChange
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("not from a nib") }

        override func becomeFirstResponder() -> Bool {
            let became = super.becomeFirstResponder()
            if became { onEditingChange(true) }
            return became
        }

        override func textDidEndEditing(_ notification: Notification) {
            super.textDidEndEditing(notification)
            onEditingChange(false)
        }
    }
}
