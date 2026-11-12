import Testing

@testable import bae

/// Unpin asks first while "Ask before unpinning" is on; the alert's answer
/// decides what is sent: the unpin alone, the setting turned off and then the
/// unpin, or nothing.
@Suite("Unpin asking first")
struct UnpinAskTests {
    @Test("it asks while the setting is on and not while it is off")
    func asksOnlyWhileTheSettingIsOn() {
        #expect(UnpinAsk.asks(askBeforeUnpinning: true))
        #expect(!UnpinAsk.asks(askBeforeUnpinning: false))
    }

    @Test("with the setting off, Unpin sends the unpin alone")
    func settingOffUnpinsAtOnce() {
        #expect(UnpinAsk.withoutAsking == [.unpin])
    }

    @Test("Unpin with the box unticked sends the unpin alone")
    func unpinSendsTheUnpin() {
        #expect(UnpinAsk.steps(after: .unpin(dontShowAgain: false)) == [.unpin])
    }

    @Test("Unpin with the box ticked turns the setting off, then unpins")
    func tickedBoxStopsAsking() {
        #expect(
            UnpinAsk.steps(after: .unpin(dontShowAgain: true))
                == [.stopAsking, .unpin]
        )
    }

    @Test("Cancel sends nothing")
    func cancelSendsNothing() {
        #expect(UnpinAsk.steps(after: .cancel).isEmpty)
    }

    @Test("the steps run in order")
    func stepsRunInOrder() async throws {
        let sent = Sent()
        try await UnpinAsk.run(
            [.stopAsking, .unpin],
            stopAsking: { await sent.add("stopAsking") },
            unpin: { await sent.add("unpin") }
        )
        #expect(await sent.calls == ["stopAsking", "unpin"])
    }

    @Test("a setting that cannot be saved leaves the releases pinned")
    func failedSettingStopsTheUnpin() async {
        struct Refused: Error {}
        let sent = Sent()
        await #expect(throws: Refused.self) {
            try await UnpinAsk.run(
                [.stopAsking, .unpin],
                stopAsking: { throw Refused() },
                unpin: { await sent.add("unpin") }
            )
        }
        #expect(await sent.calls.isEmpty)
    }

    @Test("the alert names a single release and counts several")
    func alertWording() {
        let one = PendingUnpin(releaseIds: ["a"], title: "Low Tide Hours")
        #expect(one.alertTitle == "Unpin “Low Tide Hours”?")
        let several = PendingUnpin(releaseIds: ["a", "b", "c"], title: nil)
        #expect(several.alertTitle == "Unpin 3 releases?")
        #expect(one.alertMessage != several.alertMessage)
    }

    private actor Sent {
        private(set) var calls: [String] = []
        func add(_ call: String) { calls.append(call) }
    }
}
