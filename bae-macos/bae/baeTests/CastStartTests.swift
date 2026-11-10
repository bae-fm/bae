import BaeKit
import Testing

@testable import bae

/// Starting a cast, shared by the cast picker and the Casting page.
@MainActor
@Suite("Cast start")
struct CastStartTests {
    private struct Refused: Error {}

    /// What a `Cast`'s `castTo` saw, for the test to read.
    @MainActor
    private final class Calls {
        var ended: [String] = []
        var started = 0
        var failures = 0

        func end(_ id: String) { ended.append(id) }
    }

    private func receiver(_ id: String) -> BridgeCastReceiver {
        BridgeCastReceiver(id: id, name: id, kind: .cast, casting: false)
    }

    private func settle(until done: () -> Bool) async {
        for _ in 0..<10_000 where !done() {
            await Task.yield()
        }
    }

    @Test("a cast that fails stops connecting and reports why")
    func failureStopsConnecting() async {
        let calls = Calls()
        let cast = Cast(castTo: { _ in throw Refused() })
        let start = CastStart()
        start.cast(
            to: receiver("kitchen"),
            with: cast,
            onStarted: { calls.started += 1 },
            onFailure: { error in
                #expect(error is Refused)
                calls.failures += 1
            }
        )
        #expect(start.connecting?.id == "kitchen")
        await settle { calls.failures > 0 }
        #expect(start.connecting == nil)
        #expect(calls.failures == 1)
        #expect(calls.started == 0)
    }

    @Test("a newer cast cancels the one still connecting")
    func newerCastCancelsOlder() async {
        let calls = Calls()
        let cast = Cast(castTo: { id in
            do {
                try await Task.sleep(for: .seconds(60))
            }
            catch {
                await calls.end(id)
                throw error
            }
        })
        let start = CastStart()
        start.cast(
            to: receiver("kitchen"),
            with: cast,
            onStarted: { calls.started += 1 },
            onFailure: { _ in calls.failures += 1 }
        )
        await Task.yield()
        start.cast(
            to: receiver("den"),
            with: cast,
            onStarted: { calls.started += 1 },
            onFailure: { _ in calls.failures += 1 }
        )
        await settle { !calls.ended.isEmpty }
        #expect(calls.ended == ["kitchen"])
        #expect(start.connecting?.id == "den")
        #expect(calls.started == 0)
        #expect(calls.failures == 0)
        start.cancel()
        await settle { calls.ended.count == 2 }
        #expect(start.connecting == nil)
        #expect(calls.failures == 0)
    }
}
