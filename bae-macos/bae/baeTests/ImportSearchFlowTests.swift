import BaeKit
import Foundation
import Testing

@testable import bae

/// `pickedResume` decides when a selected candidate's pane opens on the
/// identity its row already carries — the settled single match, or the choice
/// the user made before a restart — without a click. Its guards are what keep
/// the resume from firing where it would be wrong: rows past deciding,
/// folders whose identity is already settled, and prefetches that already
/// failed once.
@MainActor
struct ImportSearchFlowPickedResumeTests {
    private var candidate: Candidate {
        PreviewData.folderCandidates[0]
    }

    private let releasePick = BridgeIdentityPick.release(
        source: .musicBrainz,
        releaseId: "rel-picked",
        claim: .exact
    )

    private func pickedRow(
        for candidate: Candidate,
        placement: BridgeTriagePlacement,
        skipAction: BridgeTriageSkipAction?,
        picked: BridgeIdentityPick?
    ) -> BridgeTriageRow {
        PreviewData.triageRow(
            for: candidate,
            placement: placement,
            skipAction: skipAction,
            matched: PreviewData.triageMatch(
                releaseId: "rel-picked",
                title: "Album Title"
            ),
            selectable: false,
            picked: picked
        )
    }

    @Test
    func appliesAReadyRowsSettledPick() {
        // A settled single match is a pick identification made — the same
        // record a click writes, resumed the same way.
        let row = pickedRow(
            for: candidate,
            placement: .ready,
            skipAction: .skip,
            picked: releasePick
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: candidate, row: row)
                == releasePick
        )
    }

    @Test
    func appliesAChoiceMadeOnANeedsYouRow() {
        let row = pickedRow(
            for: candidate,
            placement: .needsYou(
                group: .pickAPressing,
                reason: .disagreement(
                    disagreement: .severalMatches(count: 4)
                )
            ),
            skipAction: .skip,
            picked: releasePick
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: candidate, row: row)
                == releasePick
        )
    }

    @Test
    func appliesAStoredUnknownChoice() {
        let row = pickedRow(
            for: candidate,
            placement: .needsYou(
                group: .noMatch,
                reason: .disagreement(disagreement: .noMatch)
            ),
            skipAction: .skip,
            picked: .unknown
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: candidate, row: row)
                == .unknown
        )
    }

    @Test
    func aRowWithNothingDecidedResumesNothing() {
        let row = pickedRow(
            for: candidate,
            placement: .needsYou(
                group: .pickAPressing,
                reason: .disagreement(
                    disagreement: .severalMatches(count: 4)
                )
            ),
            skipAction: .skip,
            picked: nil
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: candidate, row: row)
                == nil
        )
    }

    @Test
    func ignoresRowsPastDeciding() {
        // Done and Skipped rows keep their pick too, and a candidate rebuilt
        // at launch starts back with nothing settled — placement is the only
        // thing standing between an imported row and a re-opened commit-able
        // pane.
        let terminalRows: [(BridgeTriagePlacement, BridgeTriageSkipAction?)] = [
            (.done, nil),
            (.skipped, .unskip),
        ]
        for (placement, skipAction) in terminalRows {
            let row = pickedRow(
                for: candidate,
                placement: placement,
                skipAction: skipAction,
                picked: releasePick
            )
            #expect(
                ImportSearchFlow.pickedResume(candidate: candidate, row: row)
                    == nil
            )
        }
    }

    @Test
    func yieldsToAPickAlreadyIn() {
        var picked = candidate
        picked.pick = CandidatePick(
            releaseId: "rel-user-chose",
            source: .musicBrainz,
            claim: .exact
        )
        let row = pickedRow(
            for: picked,
            placement: .ready,
            skipAction: .skip,
            picked: releasePick
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: picked, row: row) == nil
        )
    }

    @Test
    func yieldsToAnIdentityAlreadySettled() {
        // A pick that resolved and a folder read as its own tags both settle
        // the identity choice; neither wants the stored pick re-applied over
        // it.
        let settled: [BridgeIdentityChoice] = [
            .exact(releaseId: "rel-picked", source: .musicBrainz),
            .unknown,
        ]
        for choice in settled {
            var advanced = candidate
            advanced.identityChoice = choice
            let row = pickedRow(
                for: advanced,
                placement: .ready,
                skipAction: .skip,
                picked: releasePick
            )
            #expect(
                ImportSearchFlow.pickedResume(candidate: advanced, row: row)
                    == nil
            )
        }
    }

    @Test
    func staysDownAfterAFailedPrefetch() {
        // Failure clears what the pick had settled and sets `error`; without
        // this guard the resume would immediately retry the same prefetch,
        // and on a persistent failure, retry it forever.
        var failed = candidate
        failed.error = "Failed to load release details"
        let row = pickedRow(
            for: failed,
            placement: .ready,
            skipAction: .skip,
            picked: releasePick
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: failed, row: row) == nil
        )
    }

    @Test
    func absentRowResumesNothing() {
        #expect(
            ImportSearchFlow.pickedResume(candidate: candidate, row: nil)
                == nil
        )
    }
}

@MainActor
@Suite("ImportSearchFlow cover selection")
struct ImportSearchFlowCoverSelectionTests {
    @Test("a release decision commits the cover shown by default")
    func releaseDecisionSelectsItsDefaultCover() async throws {
        let store = ImportStore()
        let candidate = PreviewData.folderCandidates[0]
        store.folderCandidates[candidate.key] = candidate

        let detail = PreviewData.releaseDetailBridge
        let fixture = MappingFixtures.prefetch(
            mapping: MappingFixtures.thirteenFileTable
        )
        let answer = BridgeDecidedIdentity.release(
            source: detail.source,
            releaseId: detail.releaseId,
            prefetch: BridgeReleasePrefetch(
                detail: detail,
                seed: fixture.seed,
                claim: fixture.claim,
                exactPressing: fixture.exactPressing,
                mapping: fixture.mapping
            )
        )
        let importer = Importer(candidateDecidedIdentity: { _ in answer })
        let pick = BridgeIdentityPick.release(
            source: detail.source,
            releaseId: detail.releaseId,
            claim: .exact
        )

        await ImportSearchFlow.refreshDecidedIdentity(
            importer: importer,
            importStore: store,
            key: candidate.key,
            pick: pick
        )
        .value

        let seeded = try #require(store.candidate(forKey: candidate.key))
        let expectedCover = try #require(detail.defaultCover)
        #expect(seeded.selectedCover == expectedCover)

        let request = ImportCommitRequest(
            candidate: seeded,
            storageMode: .local,
            pin: false,
            identityChoice: fixture.claim.choice,
            userEdit: nil
        )
        #expect(request.selectedCover == expectedCover.selection)
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
        store.folderCandidates[candidate.key] = candidate
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

        store.folderCandidates.removeValue(forKey: candidate.key)
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
        store.folderCandidates[candidate.key] = candidate
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
