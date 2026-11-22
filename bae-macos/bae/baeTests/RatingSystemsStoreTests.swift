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

        let following = Task { await store.follow() }
        try await Wait.until { store.systems == heartForTracks }
        following.cancel()
        await following.value

        #expect(store.systems == nil)
    }

    @Test("a choice passes the items and the system on, and shows until done")
    func choicePassesThrough() async throws {
        let calls = CallLog<String>()
        let store = store(setSystem: { items, system in
            calls.record("\(items) \(system)")
        })
        store.choose(.albums, .hidden)
        #expect(store.pending[.albums] == .hidden)
        try await Wait.until { store.pending[.albums] == nil }
        #expect(calls.all == ["albums hidden"])
    }

    @Test("a choice for items with a choice in progress is left out")
    func choiceWaitsOutAnEarlierOne() async throws {
        let calls = CallLog<String>()
        let store = store(setSystem: { items, system in
            calls.record("\(items) \(system)")
        })
        store.choose(.tracks, .stars)
        store.choose(.tracks, .hidden)
        try await Wait.until { store.pending[.tracks] == nil }
        #expect(calls.all == ["tracks stars"])
    }

    @Test("a refused change is shown and the pick goes")
    func refusedChangeIsShown() async throws {
        struct Refused: Error {}
        let log = ErrorLog()
        let store = store(
            shownErrors: log,
            setSystem: { _, _ in
                throw Refused()
            }
        )
        store.choose(.tracks, .stars)
        try await Wait.until { store.pending[.tracks] == nil }
        #expect(log.errors.count == 1)
    }
}

@MainActor
private final class ErrorLog {
    var errors: [any Error] = []
}
