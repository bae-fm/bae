import BaeKit
import Foundation
import Testing

@testable import bae

/// The three moments a typed value is sent, and the one rule that keeps a
/// field from fighting the value underneath it.
///
/// The field itself is a SwiftUI view, so what is exercised here is the
/// decision it makes: `commit` sends unless what it holds is already what is
/// stored. The debounce interval is stated once, next to the field.
@Suite("Committed text field")
struct CommittedTextFieldTests {
    @Test("the pause that counts as done typing is stated once")
    func commitDelayIsStated() {
        #expect(CommittedTextField.commitDelay == .milliseconds(400))
    }

    @MainActor
    @Test("a field that never changed sends nothing when it is left")
    func anUntouchedFieldSendsNothing() {
        var sent: [String] = []
        let field = CommittedTextField(
            placeholder: "Album title",
            value: "Album Title",
            onCommit: { sent.append($0) },
        )

        // Leaving the field with what it was handed is not an edit.
        field.commit("Album Title")
        #expect(sent.isEmpty)

        field.commit("Album Title Two")
        #expect(sent == ["Album Title Two"])
    }

    @MainActor
    @Test("clearing a field is an edit, not an absence")
    func clearingAFieldIsAnEdit() {
        var sent: [String] = []
        let field = CommittedTextField(
            placeholder: "Catalog number",
            value: "CAT-1",
            onCommit: { sent.append($0) },
        )

        field.commit("")
        #expect(sent == [""])
    }
}
