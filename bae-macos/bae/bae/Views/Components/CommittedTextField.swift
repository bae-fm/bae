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
    let placeholder: String
    /// The stored value. Re-seeds the draft whenever the field is not focused.
    let value: String
    var monospaced: Bool = false
    var boxed: Bool = true
    var font: Font = .system(size: 13)
    /// How the placeholder is drawn when set — a field that reads as a fact
    /// at rest shows its empty mark in the tertiary label color rather than
    /// the system placeholder color.
    var placeholderStyle: HierarchicalShapeStyle?
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
    @FocusState
    private var focused: Bool
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
            .modifier(FieldChrome(focused: focused, boxed: boxed))
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

    @ViewBuilder
    private var field: some View {
        let base = TextField(
            placeholder,
            text: $draft,
            prompt: placeholderStyle.map {
                Text(placeholder).foregroundStyle($0)
            }
        )
        .textFieldStyle(.plain)
        .font(font)
        .focused($focused)
        .onSubmit { startCommit(draft) }
        if monospaced {
            base.monospacedDigit()
        }
        else {
            base
        }
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
