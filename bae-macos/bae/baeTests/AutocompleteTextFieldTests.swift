import AppKit
import SwiftUI
import Testing

@testable import bae

@Suite("AutocompleteTextField")
struct AutocompleteTextFieldTests {
    @MainActor
    @Test("ASCII completion selects the appended suffix")
    func asciiCompletionSelectsSuffix() throws {
        let harness = AutocompleteHarness(
            suggestions: ["abcd"]
        )
        let field = try makeField(harness)

        complete("ab", in: field)

        #expect(field.stringValue == "abcd")
        #expect(field.currentEditor()?.selectedRange == NSRange(location: 2, length: 2))
    }

    @MainActor
    @Test("UTF-16-shorter completion inserts without crashing")
    func utf16ShorterCompletionDoesNotCrash() throws {
        let harness = AutocompleteHarness(
            suggestions: ["\u{1EAD}x"]
        )
        let field = try makeField(harness)

        complete("a\u{0323}\u{0302}", in: field)

        #expect(field.stringValue == "\u{1EAD}x")
    }

    @MainActor
    @Test("UTF-16-shorter completion leaves the caret at the inserted end")
    func utf16ShorterCompletionRangeFallsBackToInsertedEnd() {
        let range = AutocompleteTextField.completionSelectionRange(
            currentText: "a\u{0323}\u{0302}",
            match: "\u{1EAD}x"
        )

        #expect(range == NSRange(location: 2, length: 0))
    }

    @MainActor
    private func makeField(
        _ harness: AutocompleteHarness
    ) throws -> NSTextField {
        let (window, host) = SnapshotTestSupport.hostInWindow(
            AutocompleteTextField(
                text: harness.textBinding,
                placeholder: "Album Title",
                suggestions: harness.suggestions,
                isLoading: false
            ),
            size: NSSize(width: 280, height: 32)
        )
        host.layoutSubtreeIfNeeded()
        let field = try #require(host.firstDescendant(ofType: NSTextField.self))
        _ = window.makeFirstResponder(field)
        _ = try #require(field.currentEditor())
        return field
    }

    @MainActor
    private func complete(_ text: String, in field: NSTextField) {
        field.stringValue = text
        field.delegate?.controlTextDidChange?(
            Notification(name: NSControl.textDidChangeNotification, object: field)
        )
    }
}

@MainActor
private struct AutocompleteHarness {
    let suggestions: [String]
    private let holder = TextHolder()

    @MainActor
    var textBinding: Binding<String> {
        Binding(
            get: { holder.text },
            set: { holder.text = $0 }
        )
    }
}

@Observable
@MainActor
private final class TextHolder {
    var text = ""
}

extension NSView {
    func firstDescendant<View: NSView>(ofType type: View.Type) -> View? {
        if let view = self as? View {
            return view
        }
        for subview in subviews {
            if let view = subview.firstDescendant(ofType: type) {
                return view
            }
        }
        return nil
    }
}
