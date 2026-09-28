import AppKit
import Testing

@testable import bae

/// The Edit menu's clipboard items are enabled from what the first responder
/// says it can do, read again each time AppKit updates its windows.
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
        let notifications = NotificationCenter()
        let actions = FirstResponderActions(
            target: { text.responds(to: $0) ? text : nil },
            notifications: notifications
        )
        notifications.post(
            name: NSApplication.didUpdateNotification,
            object: nil
        )
        #expect(!actions.canPerform(copy))
        #expect(!actions.canPerform(cut))
        #expect(actions.canPerform(selectAll))

        text.selectAll(nil)
        #expect(!actions.canPerform(copy), "not asked again before an update")
        notifications.post(
            name: NSApplication.didUpdateNotification,
            object: nil
        )
        #expect(actions.canPerform(copy))
        #expect(actions.canPerform(cut))
    }

    @Test(
        "nothing is asked before AppKit's first update, when there may be no app yet"
    )
    func asksNothingBeforeTheFirstUpdate() {
        var asked: [Selector] = []
        let notifications = NotificationCenter()
        let actions = FirstResponderActions(
            target: {
                asked.append($0)
                return nil
            },
            notifications: notifications
        )
        #expect(asked.isEmpty)
        #expect(!actions.canPerform(selectAll))

        notifications.post(
            name: NSApplication.didUpdateNotification,
            object: nil
        )
        #expect(asked == FirstResponderActions.clipboard)
    }

    @Test("with nothing to take an action, none of them can be taken")
    func nothingTakesThem() {
        let notifications = NotificationCenter()
        let actions = FirstResponderActions(
            target: { _ in nil },
            notifications: notifications
        )
        notifications.post(
            name: NSApplication.didUpdateNotification,
            object: nil
        )
        for action in FirstResponderActions.clipboard {
            #expect(!actions.canPerform(action))
        }
    }
}
