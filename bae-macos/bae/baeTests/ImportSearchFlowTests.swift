import BaeKit
import Foundation
import Testing

@testable import bae

@MainActor
@Suite("ImportSearchFlow identity picks")
struct ImportSearchFlowIdentityTests {
    @Test("a pick that lands leaves no error and nothing in flight")
    func aPickThatLandsClearsTheFlag() async throws {
        let store = ImportStore()
        let candidate = PreviewData.folderCandidates[0]
        store.selectedCandidates[candidate.key] = candidate
        var picked: [BridgeIdentityPick] = []
        let importer = Importer(
            pickCandidateIdentity: { _, pick in
                await MainActor.run { picked.append(pick) }
            }
        )
        let pick = BridgeIdentityPick.release(
            source: .musicBrainz,
            releaseId: "rel-picked"
        )

        ImportSearchFlow.decideIdentity(
            importer: importer,
            importStore: store,
            key: candidate.key,
            pick: pick
        )
        try await Task.sleep(for: .milliseconds(50))

        #expect(picked == [pick])
        let after = try #require(store.candidate(forKey: candidate.key))
        #expect(after.error == nil)
        #expect(after.pickInFlight == false)
    }

    @Test("a pick that fails states the failure and stores nothing")
    func aPickThatFailsStatesIt() async throws {
        let store = ImportStore()
        let candidate = PreviewData.folderCandidates[0]
        store.selectedCandidates[candidate.key] = candidate
        let importer = Importer(
            pickCandidateIdentity: { _, _ in throw StubError.notImplemented }
        )

        ImportSearchFlow.decideIdentity(
            importer: importer,
            importStore: store,
            key: candidate.key,
            pick: .unknown
        )
        try await Task.sleep(for: .milliseconds(50))

        let after = try #require(store.candidate(forKey: candidate.key))
        #expect(after.error != nil)
        #expect(after.pickInFlight == false)
        // Nothing about the pane moved: the pick never landed, so there is
        // nothing new for it to draw.
        #expect(after.detail == nil)
    }
}

@MainActor
@Suite("ImportSearchFlow cover selection")
struct ImportSearchFlowCoverSelectionTests {
    @Test("a picked candidate shows the cover core answers with")
    func aPickedCandidateShowsItsCover() throws {
        let store = ImportStore()
        var detail = MappingFixtures.detail(
            mapping: MappingFixtures.thirteenFileTable
        )
        let cover = try #require(PreviewData.releaseDetailBridge.defaultCover)
        detail.cover = cover
        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: detail
        )

        let seeded = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(seeded.cover == cover)
        // And the sidebar reads the same one.
        #expect(
            store.sidebarCover(for: detail.row) == cover.thumbnailContent
        )
    }
}

@MainActor
@Suite("ImportSearchFlow live library status")
struct ImportSearchFlowLibraryStatusTests {
    @Test(
        "a search result keeps its library status live until the candidate closes"
    )
    func resultStatusUpdatesAndCancels() async throws {
        let store = ImportStore()
        let candidate = PreviewData.folderCandidates[0]
        store.selectedCandidates[candidate.key] = candidate
        let harness = ReleaseStatusHarness()
        let response = searchResponse()
        let importer = Importer(
            searchForCandidate: { _ in response },
            subscribeReleaseLibraryStatus: harness.subscribe
        )

        ImportSearchFlow.dispatchSearch(
            importer: importer,
            importStore: store,
            key: candidate.key
        )
        await waitUntil { harness.callback(releaseId: "rel-live") != nil }

        try #require(harness.callback(releaseId: "rel-live"))
            .onValue(
                value: BridgeLibraryStatus(
                    releaseId: "rel-live",
                    releaseInLibrary: true,
                    albumInLibrary: true,
                    albumTitle: "Album Title",
                    albumId: "album-live"
                )
            )
        await waitUntil {
            store.candidate(forKey: candidate.key)?
                .libraryStatuses["rel-live"]?
                .albumId == "album-live"
        }

        store.selectedCandidates.removeValue(forKey: candidate.key)
        #expect(harness.subscription(releaseId: "rel-live")?.cancelled == true)
    }

    @Test("an old same-key status subscription cannot update its replacement")
    func sameKeyReplacementRejectsOldCallbacks() async throws {
        let store = ImportStore()
        var candidate = PreviewData.folderCandidates[0]
        let statusKey = ReleaseLibraryStatusSubscriptionKey(
            source: .musicBrainz,
            releaseId: "rel-live",
            sourceGroupId: "group-live"
        )
        var results = candidate.search.activeResults()
        results.libraryStatusSubscriptionKeys = [statusKey]
        candidate.search.setResults(
            results,
            forTab: candidate.search.activeTab,
            source: candidate.search.activeSource
        )
        store.selectedCandidates[candidate.key] = candidate
        let harness = ReleaseStatusHarness()
        let importer = Importer(
            subscribeReleaseLibraryStatus: harness.subscribe
        )

        store.refreshLibraryStatusSubscriptions(
            importer: importer,
            key: candidate.key
        )
        await waitUntil { harness.callbackCount(releaseId: "rel-live") == 1 }

        store.mutateCandidate(forKey: candidate.key) { current in
            var empty = current.search.activeResults()
            empty.libraryStatusSubscriptionKeys = []
            current.search.setResults(
                empty,
                forTab: current.search.activeTab,
                source: current.search.activeSource
            )
        }
        store.refreshLibraryStatusSubscriptions(
            importer: importer,
            key: candidate.key
        )
        store.mutateCandidate(forKey: candidate.key) { current in
            var restored = current.search.activeResults()
            restored.libraryStatusSubscriptionKeys = [statusKey]
            current.search.setResults(
                restored,
                forTab: current.search.activeTab,
                source: current.search.activeSource
            )
        }
        store.refreshLibraryStatusSubscriptions(
            importer: importer,
            key: candidate.key
        )
        await waitUntil { harness.callbackCount(releaseId: "rel-live") == 2 }

        try #require(harness.callback(releaseId: "rel-live", index: 0))
            .onValue(
                value: BridgeLibraryStatus(
                    releaseId: "rel-live",
                    releaseInLibrary: true,
                    albumInLibrary: true,
                    albumTitle: "Old Title",
                    albumId: "album-old"
                )
            )
        await Task.yield()
        #expect(
            store.candidate(forKey: candidate.key)?
                .libraryStatuses["rel-live"] == nil
        )

        try #require(harness.callback(releaseId: "rel-live", index: 1))
            .onValue(
                value: BridgeLibraryStatus(
                    releaseId: "rel-live",
                    releaseInLibrary: true,
                    albumInLibrary: true,
                    albumTitle: "New Title",
                    albumId: "album-new"
                )
            )
        await waitUntil {
            store.candidate(forKey: candidate.key)?
                .libraryStatuses["rel-live"]?
                .albumId == "album-new"
        }
    }

    private func searchResponse() -> BridgeCandidateSearchResults {
        BridgeCandidateSearchResults(
            tab: .general,
            source: .musicBrainz,
            groups: [
                BridgeReleaseGroup(
                    id: "group-live",
                    sourceGroupId: "group-live",
                    title: "Album Title",
                    artist: "Artist Name",
                    coverArt: nil,
                    sourceLabel: "MusicBrainz",
                    groupUrl: "https://example.invalid/group-live",
                    yearMin: 2000,
                    yearMax: 2000,
                    pressings: [
                        BridgeMetadataResult(
                            source: .musicBrainz,
                            releaseId: "rel-live",
                            year: 2000,
                            format: "CD",
                            label: nil,
                            catalogNumber: nil,
                            country: nil
                        )
                    ]
                )
            ],
            statuses: []
        )
    }

    private func waitUntil(_ predicate: () -> Bool) async {
        for _ in 0..<100 where !predicate() {
            await Task.yield()
        }
        #expect(predicate())
    }
}

private final class ReleaseStatusHarness: @unchecked Sendable {
    private let lock = NSLock()
    private var callbacks: [String: [ReleaseLibraryStatusCallback]] = [:]
    private var subscriptions: [String: [TestReleaseStatusSubscription]] = [:]

    func subscribe(
        _ source: BridgeMetadataSource,
        _ releaseId: String,
        _ sourceGroupId: String?,
        _ callback: ReleaseLibraryStatusCallback
    ) -> any LiveSubscriptionProtocol {
        let subscription = TestReleaseStatusSubscription()
        lock.withLock {
            callbacks[releaseId, default: []].append(callback)
            subscriptions[releaseId, default: []].append(subscription)
        }
        return subscription
    }

    func callback(releaseId: String) -> ReleaseLibraryStatusCallback? {
        callback(releaseId: releaseId, index: 0)
    }

    func callback(
        releaseId: String,
        index: Int
    ) -> ReleaseLibraryStatusCallback? {
        lock.withLock { callbacks[releaseId]?[index] }
    }

    func callbackCount(releaseId: String) -> Int {
        lock.withLock { callbacks[releaseId]?.count ?? 0 }
    }

    func subscription(releaseId: String) -> TestReleaseStatusSubscription? {
        lock.withLock { subscriptions[releaseId]?.first }
    }
}

private final class TestReleaseStatusSubscription: LiveSubscriptionProtocol,
    @unchecked Sendable
{
    private let lock = NSLock()
    private(set) var cancelled = false

    func cancel() {
        lock.withLock { cancelled = true }
    }
}
