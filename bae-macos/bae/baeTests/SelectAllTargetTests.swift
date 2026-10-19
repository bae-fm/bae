import AppKit
import Testing

@testable import bae

@MainActor
@Suite("Select All target")
struct SelectAllTargetTests {
    /// The Select All a focused list offers runs in place of the table's;
    /// anywhere else Select All goes up the responder chain, enabled as the
    /// responder that takes it says.
    @Test("Select All runs the focused list's own, else the responder chain's")
    func selectAllTarget() {
        let offer = ShownRowsSelectAll()
        let text = NSTextView()
        text.string = "Album Title"
        let sent = CallLog<Bool>()
        let target = SelectAllTarget(
            focusedList: { offer },
            responder: { _ in text },
            send: { _ in
                sent.record(true)
                return true
            }
        )
        let item = NSMenuItem(
            title: "Select All",
            action: #selector(NSResponder.selectAll(_:)),
            keyEquivalent: "a"
        )

        #expect(target.validateMenuItem(item))
        target.selectAll(item)
        #expect(sent.all.count == 1)

        var ran = 0
        offer.offered = { ran += 1 }
        #expect(target.validateMenuItem(item))
        target.selectAll(item)
        #expect(ran == 1)
        #expect(sent.all.count == 1)

        let nothing = SelectAllTarget(
            focusedList: { nil },
            responder: { _ in nil },
            send: { _ in false }
        )
        #expect(!nothing.validateMenuItem(item))
    }
}
