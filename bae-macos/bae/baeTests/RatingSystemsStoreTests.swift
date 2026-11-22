import BaeKit
import Foundation
import Testing

@testable import bae

@MainActor
@Suite("Rating systems", .serialized)
struct RatingSystemsStoreTests {
    private let heartForTracks = BridgeRatingSystems(
        albums: .stars,
        tracks: .heart
    )

    private func store(
        shownErrors: ErrorLog = ErrorLog(),
        setSystem:
            @escaping @Sendable (BridgeRatedItems, BridgeRatingSystem)
            async throws -> Void = { _, _ in }
    ) -> RatingSystemsStore {
        let systems = heartForTracks
        return RatingSystemsStore(
            subscribe: { PulledValues(FixedSubscription(value: systems)) },
            setSystem: setSystem,
            showError: { shownErrors.errors.append($0) }
        )
    }

    @Test("the systems are read while a screen follows them, and go with it")
    func readWhileFollowed() async throws {
        let store = store()
        #expect(store.systems == nil)
        #expect(
            store.shown == BridgeRatingSystems(albums: .hidden, tracks: .hidden)
        )

        let following = Task { await store.follow() }
        try await Wait.until { store.systems == heartForTracks }
        following.cancel()
        await following.value

        #expect(store.systems == nil)
    }

    @Test("a change passes the items and the system on")
    func changePassesThrough() async {
        let calls = CallLog()
        let store = store(setSystem: { items, system in
            await calls.record("\(items) \(system)")
        })
        await store.set(.albums, to: .hidden)
        #expect(await calls.entries == ["albums hidden"])
    }

    @Test("a refused change is shown")
    func refusedChangeIsShown() async {
        struct Refused: Error {}
        let log = ErrorLog()
        let store = store(
            shownErrors: log,
            setSystem: { _, _ in
                throw Refused()
            }
        )
        await store.set(.tracks, to: .stars)
        #expect(log.errors.count == 1)
    }
}

@MainActor
private final class ErrorLog {
    var errors: [any Error] = []
}

private actor CallLog {
    private(set) var entries: [String] = []
    func record(_ entry: String) { entries.append(entry) }
}
