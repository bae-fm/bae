import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing
import XCTest

@testable import bae

@MainActor
final class SettingsNavigationTests: XCTestCase {
    func testOpeningDiscogsSelectsItsPaneBeforePresentation() {
        let navigation = SettingsNavigation()
        var selectionAtPresentation: SettingsTab?

        navigation.open(.discogs) {
            selectionAtPresentation = navigation.selectedTab
        }

        #expect(selectionAtPresentation == .discogs)
    }

    func testDiscogsKeyFieldTakesFocusWhenItAppears() async {
        let size = NSSize(width: 500, height: 320)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            DiscogsSettingsContent(
                draft: .constant(""),
                status: .notConfigured,
                isValidating: false,
                saveError: nil,
                readError: nil,
                onSave: {},
                onRecheck: {},
                onRemove: {}
            )
            .frame(width: size.width, height: size.height),
            size: size
        )

        await Task.yield()
        host.layoutSubtreeIfNeeded()
        await Task.yield()

        let textFields = SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSTextField }
        XCTAssertTrue(
            textFields.contains { $0.currentEditor() === window.firstResponder }
        )
        withExtendedLifetime(window) {}
    }
}

@MainActor
final class ManualSearchWorkspaceTests: XCTestCase {
    func testIdleManualSearchHasNoResultsScroller() async {
        let state = ImportSearchState(
            identifyState: .idle,
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: true,
            signals: nil,
            signalsToolbar: BridgeSignalsToolbar(signals: [])
        )
        let size = NSSize(width: 900, height: 600)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportSearchPane.preview(state: state, mode: .manual)
                .frame(width: size.width, height: size.height),
            size: size
        )

        await Task.yield()
        host.layoutSubtreeIfNeeded()

        XCTAssertFalse(
            SnapshotTestSupport.descendants(of: host)
                .contains { $0 is NSScrollView }
        )
        withExtendedLifetime(window) {}
    }
}

@MainActor
@Suite("Find online method presentation")
struct FindOnlineMethodPresentationTests {
    @Test("idle automatic method has no empty-result message")
    func idleAutomaticMethodHasNoEmptyResultMessage() async {
        let labels = await labels(
            for: ImportSearchPane.preview(state: idleState)
        )

        #expect(labels.contains(String(localized: "Automatic")))
        #expect(labels.contains(String(localized: "Search manually")))
        #expect(labels.contains(String(localized: "Identify automatically")))
        #expect(
            !labels.contains(String(localized: "No automatic matches found"))
        )
    }

    @Test("manual method has no empty-result message before a search")
    func manualMethodHasNoEmptyResultMessageBeforeSearch() async {
        let labels = await labels(
            for: ImportSearchPane.preview(state: idleState, mode: .manual)
        )

        #expect(!labels.contains(String(localized: "No results")))
        #expect(!labels.contains(String(localized: "No matches found")))
    }

    private var idleState: ImportSearchState {
        ImportSearchState(
            identifyState: .idle,
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: true,
            signals: nil,
            signalsToolbar: BridgeSignalsToolbar(signals: [])
        )
    }

    private func labels<V: View>(for view: V) async -> [String] {
        let size = NSSize(width: 900, height: 600)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            view.frame(width: size.width, height: size.height),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let labels = SnapshotTestSupport.descendants(of: host)
            .compactMap { ($0 as? NSTextField)?.stringValue }
        withExtendedLifetime(window) {}
        return labels
    }
}

@MainActor
final class SignalBadgePopoverTests: XCTestCase {
    func testFailedLookupOffersRetryBesideItsSignalValue() async throws {
        var didRetry = false
        let signal = BridgeToolbarSignal(
            kind: .barcode,
            value: "0123456789012",
            origin: .artwork,
            state: .failed(failure: .network),
            excluded: false,
            options: []
        )
        let rendered = await render(
            SignalBadgePopover(signal: signal, onRetry: { didRetry = true })
        )

        XCTAssertTrue(rendered.labels.contains("0123456789012"))
        let retryButton = try XCTUnwrap(rendered.buttons.first)
        retryButton.performClick(nil)
        XCTAssertTrue(didRetry)
    }

    private func render<V: View>(_ view: V) async -> (
        labels: [String],
        buttons: [NSButton]
    ) {
        let size = NSSize(width: 300, height: 220)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            view.frame(width: size.width, height: size.height),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let descendants = SnapshotTestSupport.descendants(of: host)
        let labels = descendants.compactMap {
            ($0 as? NSTextField)?.stringValue
        }
        let buttons = descendants.compactMap { $0 as? NSButton }
        withExtendedLifetime(window) {}
        return (labels, buttons)
    }
}

@MainActor
@Suite("ImportSearchFlow metadata application")
struct ImportSearchFlowMetadataApplicationTests {
    @Test("command return keeps the sheet open until detail delivery")
    func commandReturnWaitsForDetailDelivery() async throws {
        let store = unsettledStore()
        let uiStore = UiStore()
        let presentation = presentModal(in: uiStore)
        let recorder = PickRecorder()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, source, releaseId in
                await recorder.record(
                    .externalRelease(source: source, releaseId: releaseId)
                )
                return 1
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
                .metadataApplicationSession?
                .commandRevision == 1
        }

        #expect(recorder.provenances == [MappingFixtures.provenance])
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
        #expect(delivered.provenanceInFlight == nil)
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
            applyCandidateExternalMetadata: { _, source, releaseId in
                await recorder.record(
                    .externalRelease(source: source, releaseId: releaseId)
                )
                for await _ in gate { break }
                return 1
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
            recorder.provenances == [MappingFixtures.provenance]
        }

        deliverPickedDetail(to: store)

        #expect(uiStore.modalPresentation === presentation)
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .provenanceInFlight == MappingFixtures.provenance
        )

        releaseGate.finish()
        await waitUntil { uiStore.modalBuilder == nil }
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .provenanceInFlight == nil
        )
    }

    @Test("a different detail cannot confirm the chosen pressing")
    func mismatchedDetailCannotConfirmTheChoice() async throws {
        let store = unsettledStore()
        let uiStore = UiStore()
        let presentation = presentModal(in: uiStore)
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _, _ in 1 }
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
                .metadataApplicationSession?
                .commandRevision == 1
        }

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.fileTagsTable,
                metadataProvenance: .externalRelease(
                    source: MappingFixtures.source,
                    releaseId: "rel-other"
                )
            )
        )

        #expect(uiStore.modalPresentation === presentation)
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .provenanceInFlight == MappingFixtures.provenance
        )

        deliverPickedDetail(to: store)
        #expect(uiStore.modalBuilder == nil)
    }

    @Test(
        "an older revision from the same release cannot confirm reapplication"
    )
    func olderSameReleaseRevisionCannotConfirm() async throws {
        let store = unsettledStore()
        let uiStore = UiStore()
        let presentation = presentModal(in: uiStore)
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _, _ in 2 }
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
                .metadataApplicationSession?
                .commandRevision == 2
        }

        deliverPickedDetail(to: store, revision: 1)
        #expect(uiStore.modalPresentation === presentation)

        deliverPickedDetail(to: store, revision: 2)
        #expect(uiStore.modalBuilder == nil)
    }

    @Test("a failed choice leaves its sheet open and shows the error")
    func failedChoiceLeavesSheetOpenAndShowsError() async throws {
        let store = unsettledStore()
        let uiStore = UiStore()
        let presentation = presentModal(in: uiStore)
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _, _ in
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
        #expect(after.provenanceInFlight == nil)
        #expect(after.pickedRelease == nil)
        #expect(uiStore.modalPresentation === presentation)
    }

    @Test("a late choice cannot dismiss a newer modal")
    func lateChoiceCannotDismissNewerModal() async {
        let store = unsettledStore()
        let uiStore = UiStore()
        let searchPresentation = presentModal(in: uiStore)
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _, _ in 1 }
        )

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
                .metadataApplicationSession?
                .commandRevision == 1
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
            detail: MappingFixtures.detail(
                mapping: nil,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil
            )
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

    private func deliverPickedDetail(
        to store: ImportStore,
        revision: UInt64 = 1
    ) {
        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataRevision: revision
            )
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
    var provenances: [BridgeMetadataProvenance] = []

    func record(_ provenance: BridgeMetadataProvenance) {
        provenances.append(provenance)
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
