import BaeKit
import SwiftUI

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
    /// Send the typed value to wherever it lives.
    let onCommit: (String) -> Void

    /// How long a pause counts as "done typing".
    static let commitDelay: Duration = .milliseconds(400)

    @State
    private var draft: String = ""
    @State
    private var pending: Task<Void, Never>?
    @FocusState
    private var focused: Bool

    var body: some View {
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
                    commit(next)
                }
            }
            .onChange(of: focused) { _, isFocused in
                guard !isFocused else { return }
                commit(draft)
            }
    }

    @ViewBuilder
    private var field: some View {
        let base = TextField(placeholder, text: $draft)
            .textFieldStyle(.plain)
            .font(.system(size: 13))
            .focused($focused)
            .onSubmit { commit(draft) }
        if monospaced {
            base.monospacedDigit()
        }
        else {
            base
        }
    }

    /// Send `text` unless it is already what is stored — a focus change over
    /// an untouched field is not an edit.
    func commit(_ text: String) {
        pending?.cancel()
        pending = nil
        guard text != value else { return }
        onCommit(text)
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
