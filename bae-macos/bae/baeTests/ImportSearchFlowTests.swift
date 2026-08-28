import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

@MainActor
@Suite("ImportSearchFlow identity picks")
struct ImportSearchFlowIdentityTests {
    @Test("command return keeps the sheet open until detail delivery")
    func commandReturnWaitsForDetailDelivery() async throws {
        let store = unsettledStore()
        let uiStore = UiStore()
        let presentation = presentModal(in: uiStore)
        let recorder = PickRecorder()
        let importer = Importer(
            selectCandidateMetadataSeed: { _, seed in
                await recorder.record(seed)
            }
        )

        ImportSearchFlow.chooseReleaseFromSearchSheet(
            result(),
            importer: importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            onConfirmed: { uiStore.dismissModal(presentation) }
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataSeedSession?
                .commandSucceeded == true
        }

        #expect(recorder.seeds == [MappingFixtures.seed])
        #expect(uiStore.modalPresentation === presentation)
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .loadingReleaseId == MappingFixtures.releaseId
        )

        deliverPickedDetail(to: store)

        let delivered = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(delivered.error == nil)
        #expect(delivered.pickedRelease?.releaseId == MappingFixtures.releaseId)
        #expect(delivered.seedInFlight == nil)
        #expect(uiStore.modalBuilder == nil)
    }

    @Test("detail delivery before command return still waits for both")
    func detailDeliveryBeforeCommandReturnWaitsForBoth() async throws {
        let store = unsettledStore()
        let uiStore = UiStore()
        let presentation = presentModal(in: uiStore)
        let (gate, releaseGate) = AsyncStream<Void>.makeStream()
        let recorder = PickRecorder()
        let importer = Importer(
            selectCandidateMetadataSeed: { _, seed in
                await recorder.record(seed)
                for await _ in gate { break }
            }
        )

        ImportSearchFlow.chooseReleaseFromSearchSheet(
            result(),
            importer: importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            onConfirmed: { uiStore.dismissModal(presentation) }
        )
        await waitUntil { recorder.seeds == [MappingFixtures.seed] }

        deliverPickedDetail(to: store)

        #expect(uiStore.modalPresentation === presentation)
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .seedInFlight == MappingFixtures.seed
        )

        releaseGate.finish()
        await waitUntil { uiStore.modalBuilder == nil }
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .seedInFlight == nil
        )
    }

    @Test("a different detail cannot confirm the chosen pressing")
    func mismatchedDetailCannotConfirmTheChoice() async throws {
        let store = unsettledStore()
        let uiStore = UiStore()
        let presentation = presentModal(in: uiStore)
        let importer = Importer()

        ImportSearchFlow.chooseReleaseFromSearchSheet(
            result(),
            importer: importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            onConfirmed: { uiStore.dismissModal(presentation) }
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataSeedSession?
                .commandSucceeded == true
        }

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.unknownTable,
                metadataSeed: .externalRelease(
                    source: MappingFixtures.source,
                    releaseId: "rel-other"
                )
            )
        )

        #expect(uiStore.modalPresentation === presentation)
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .seedInFlight == MappingFixtures.seed
        )

        deliverPickedDetail(to: store)
        #expect(uiStore.modalBuilder == nil)
    }

    @Test("a failed choice leaves its sheet open and shows the error")
    func failedChoiceLeavesSheetOpenAndShowsError() async throws {
        let store = unsettledStore()
        let uiStore = UiStore()
        let presentation = presentModal(in: uiStore)
        let importer = Importer(
            selectCandidateMetadataSeed: { _, _ in
                throw StubError.notImplemented
            }
        )

        ImportSearchFlow.chooseReleaseFromSearchSheet(
            result(),
            importer: importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            onConfirmed: { uiStore.dismissModal(presentation) }
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?.error != nil
        }

        let after = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(after.error != nil)
        #expect(after.seedInFlight == nil)
        #expect(after.pickedRelease == nil)
        #expect(uiStore.modalPresentation === presentation)
    }

    @Test("a late choice cannot dismiss a newer modal")
    func lateChoiceCannotDismissNewerModal() async {
        let store = unsettledStore()
        let uiStore = UiStore()
        let searchPresentation = presentModal(in: uiStore)
        let importer = Importer()

        ImportSearchFlow.chooseReleaseFromSearchSheet(
            result(),
            importer: importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            onConfirmed: {
                uiStore.dismissModal(searchPresentation)
            }
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataSeedSession?
                .commandSucceeded == true
        }
        uiStore.dismissModal(searchPresentation)
        let newerPresentation = presentModal(in: uiStore)

        deliverPickedDetail(to: store)

        #expect(uiStore.modalPresentation === newerPresentation)
        #expect(uiStore.modalBuilder != nil)
    }

    private func unsettledStore() -> ImportStore {
        let store = ImportStore()
        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(mapping: nil, metadataSeed: nil)
        )
        return store
    }

    private func result() -> BridgeMetadataResult {
        BridgeMetadataResult(
            source: MappingFixtures.source,
            releaseId: MappingFixtures.releaseId,
            year: 1996,
            format: "CD",
            label: "Label Name",
            catalogNumber: "CAT-001",
            country: "US"
        )
    }

    private func presentModal(in uiStore: UiStore) -> ModalPresentation {
        let presentation = ModalPresentation()
        uiStore.presentModal(presentation: presentation) { EmptyView() }
        return presentation
    }

    private func deliverPickedDetail(to store: ImportStore) {
        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(mapping: nil)
        )
    }

    private func waitUntil(_ predicate: () -> Bool) async {
        for _ in 0..<100 where !predicate() {
            await Task.yield()
        }
        #expect(predicate())
    }
}

@MainActor
private final class PickRecorder {
    var seeds: [BridgeMetadataSeed] = []

    func record(_ seed: BridgeMetadataSeed) {
        seeds.append(seed)
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
        let (candidate, statusKey) = candidateWithStatusSubscription()
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

        try deliverStatus(harness, index: 0, albumId: "album-old")
        await Task.yield()
        #expect(
            store.candidate(forKey: candidate.key)?
                .libraryStatuses["rel-live"] == nil
        )

        try deliverStatus(harness, index: 1, albumId: "album-new")
        await waitUntil {
            store.candidate(forKey: candidate.key)?
                .libraryStatuses["rel-live"]?
                .albumId == "album-new"
        }
    }

    private func candidateWithStatusSubscription() -> (
        Candidate,
        ReleaseLibraryStatusSubscriptionKey
    ) {
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
        return (candidate, statusKey)
    }

    private func deliverStatus(
        _ harness: ReleaseStatusHarness,
        index: Int,
        albumId: String
    ) throws {
        try #require(harness.callback(releaseId: "rel-live", index: index))
            .onValue(
                value: BridgeLibraryStatus(
                    releaseId: "rel-live",
                    releaseInLibrary: true,
                    albumInLibrary: true,
                    albumTitle: "Album Title",
                    albumId: albumId
                )
            )
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
