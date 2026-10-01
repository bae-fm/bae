import AppKit
import Testing

@testable import bae

/// The Edit menu's clipboard items are enabled from what the first responder
/// says it can do when asked.
@MainActor
@Suite("What the first responder can do with the clipboard items")
struct FirstResponderActionsTests {
    private let copy = #selector(NSText.copy(_:))
    private let cut = #selector(NSText.cut(_:))
    private let selectAll = #selector(NSText.selectAll(_:))

    @Test("a text view with nothing selected can't copy, and can once it has")
    func followsTheTextSelection() {
        let text = NSTextView()
        text.string = "Album Title"
        text.setSelectedRange(NSRange(location: text.string.count, length: 0))
        let actions = FirstResponderActions(
            target: { text.responds(to: $0) ? text : nil }
        )
        let before = actions.performable()
        #expect(!before.contains(copy))
        #expect(!before.contains(cut))
        #expect(before.contains(selectAll))

        text.selectAll(nil)
        let after = actions.performable()
        #expect(after.contains(copy))
        #expect(after.contains(cut))
    }

    @Test("every clipboard action is asked about")
    func asksAboutEveryClipboardAction() {
        var asked: [Selector] = []
        let actions = FirstResponderActions(
            target: {
                asked.append($0)
                return nil
            }
        )
        _ = actions.performable()
        #expect(asked == FirstResponderActions.clipboard)
    }

    @Test("with nothing to take an action, none of them can be taken")
    func nothingTakesThem() {
        let actions = FirstResponderActions(target: { _ in nil })
        #expect(actions.performable().isEmpty)
    }
}
