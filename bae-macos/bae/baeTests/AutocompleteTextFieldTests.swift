import AppKit
import SwiftUI
import Testing

@testable import bae

@Suite("AutocompleteTextField")
struct AutocompleteTextFieldTests {
    @MainActor
    @Test("ASCII completion selects the appended suffix")
    func asciiCompletionSelectsSuffix() async throws {
        let harness = AutocompleteHarness(
            suggestions: ["abcd"]
        )
        try await withField(harness) { field in
            complete("ab", in: field)

            #expect(field.stringValue == "abcd")
            #expect(
                field.currentEditor()?.selectedRange
                    == NSRange(location: 2, length: 2)
            )
        }
    }

    @MainActor
    @Test("UTF-16-shorter completion inserts without crashing")
    func utf16ShorterCompletionDoesNotCrash() async throws {
        let harness = AutocompleteHarness(
            suggestions: ["\u{1EAD}x"]
        )
        try await withField(harness) { field in
            complete("a\u{0323}\u{0302}", in: field)

            #expect(field.stringValue == "\u{1EAD}x")
        }
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

    /// The field hosted and being edited, for the length of `body`.
    @MainActor
    private func withField(
        _ harness: AutocompleteHarness,
        _ body: (NSTextField) throws -> Void
    ) async throws {
        try await SnapshotTestSupport.withHostedWindow(
            AutocompleteTextField(
                text: harness.textBinding,
                placeholder: "Album Title",
                suggestions: harness.suggestions,
                isLoading: false
            ),
            size: NSSize(width: 280, height: 32)
        ) { window, host in
            host.layoutSubtreeIfNeeded()
            let field = try #require(
                host.firstDescendant(ofType: NSTextField.self)
            )
            HostedInput.focus(field, in: window)
            _ = try #require(field.currentEditor())
            try body(field)
        }
    }

    @MainActor
    private func complete(_ text: String, in field: NSTextField) {
        field.stringValue = text
        field.delegate?.controlTextDidChange?(
            Notification(
                name: NSControl.textDidChangeNotification,
                object: field
            )
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
