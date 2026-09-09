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

        await SnapshotTestSupport.settle(host)

        let textFields = SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSTextField }
        XCTAssertTrue(
            textFields.contains { $0.currentEditor() === window.firstResponder }
        )
        withExtendedLifetime(window) {}
    }
}

@MainActor
@Suite("ImportSearchFlow metadata application")
struct ImportSearchFlowMetadataApplicationTests {
    @Test("the read landing puts the draft back in the metadata slot")
    func theReadLandingReturnsToTheDraft() async throws {
        let writes = SessionWriteRecorder()
        let store = unsettledStore(writes: writes)
        let recorder = PickRecorder()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, provenance in
                await recorder.record(provenance)
                return 1
            }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: MappingFixtures.provenance
        )
        await waitUntil {
            writes.presentations(forKey: MappingFixtures.candidateKey)
                == [.draft]
        }

        #expect(recorder.provenances == [MappingFixtures.provenance])
        #expect(
            store.metadataApplicationSession(
                forKey: MappingFixtures.candidateKey
            ) == nil
        )
    }

    @Test("the row spins on the pressing being read until the read lands")
    func theRowSpinsWhileTheReadRuns() async throws {
        let store = unsettledStore()
        let (gate, releaseGate) = AsyncStream<Void>.makeStream()
        let recorder = PickRecorder()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, provenance in
                await recorder.record(provenance)
                for await _ in gate { break }
                return 1
            }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: MappingFixtures.provenance
        )
        await waitUntil {
            recorder.provenances == [MappingFixtures.provenance]
        }

        #expect(
            store.loadingReleaseId(forKey: MappingFixtures.candidateKey)
                == MappingFixtures.releaseId
        )

        releaseGate.finish()
        await waitUntil {
            store.loadingReleaseId(forKey: MappingFixtures.candidateKey) == nil
        }
    }

    /// The candidate is re-read whenever anything about it moves — another
    /// pane's write, the scan, the run. None of that is this pick's answer.
    @Test("a candidate re-read does not end the pick")
    func aCandidateReReadDoesNotEndThePick() async throws {
        let store = unsettledStore()
        let (gate, releaseGate) = AsyncStream<Void>.makeStream()
        let recorder = PickRecorder()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, provenance in
                await recorder.record(provenance)
                for await _ in gate { break }
                return 1
            }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: MappingFixtures.provenance
        )
        await waitUntil {
            recorder.provenances == [MappingFixtures.provenance]
        }

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(mapping: nil)
        )

        #expect(
            store.metadataApplicationSession(
                forKey: MappingFixtures.candidateKey
            ) != nil
        )
        releaseGate.finish()
    }

    @Test("a failed read keeps its error on the release, not on the pane")
    func failedReadKeepsItsErrorOnTheRelease() async throws {
        let writes = SessionWriteRecorder()
        let store = unsettledStore(writes: writes)
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _ in
                throw StubError.notImplemented
            }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: MappingFixtures.provenance
        )
        await waitUntil {
            store.releaseSelectionFailure(
                forKey: MappingFixtures.candidateKey
            ) != nil
        }

        let after = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(after.pickedRelease == nil)
        #expect(after.error == nil)
        #expect(
            store.releaseSelectionFailure(
                forKey: MappingFixtures.candidateKey
            )?
            .release.releaseId == MappingFixtures.releaseId
        )
        #expect(
            !writes.errors(forKey: MappingFixtures.candidateKey)
                .contains { $0 != nil }
        )
        #expect(
            writes.presentations(forKey: MappingFixtures.candidateKey).isEmpty
        )
    }

    private func unsettledStore(
        writes: SessionWriteRecorder? = nil
    ) -> ImportStore {
        let store = ImportStore()
        if let writes {
            store.sessionWriter = .recording { writes.record($0) }
        }
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

    private func waitUntil(_ predicate: () -> Bool) async {
        for _ in 0..<100 where !predicate() {
            await Task.yield()
        }
        #expect(predicate())
    }
}

@MainActor
final class MetadataApplicationEditingTests: XCTestCase {
    func testApplyingMetadataReplacesTheFocusedFieldForEverySource()
        async throws
    {
        for provenance in [
            BridgeMetadataProvenance.fileTags,
            .externalRelease(
                source: .musicBrainz,
                releaseId: "release-mb",
                partners: []
            ),
            .externalRelease(
                source: .discogs,
                releaseId: "release-discogs",
                partners: []
            ),
        ] {
            try await assertFocusedFieldIsReplaced(by: provenance)
        }
    }

    private func assertFocusedFieldIsReplaced(
        by provenance: BridgeMetadataProvenance
    ) async throws {
        let model = MetadataApplicationEditingModel()
        let editingCommands = EditingCommitCommands()
        let size = NSSize(width: 700, height: 560)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ReleaseMetadataHeader(
                values: model.edit,
                writer: ReleaseFieldWriter(
                    setField: { field, value in
                        model.commit(field: field, value: value)
                    }
                ),
                editingCommands: editingCommands,
                cover: { EmptyView() },
                context: { EmptyView() },
                sourceAudio: { EmptyView() }
            )
            .environment(Library.stub())
            .environment(UiStore())
            .frame(width: size.width, height: size.height),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await SnapshotTestSupport.settle(host)
        host.layoutSubtreeIfNeeded()

        let titleField = try XCTUnwrap(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
                .first { $0.stringValue == model.originalTitle }
        )
        XCTAssertTrue(window.makeFirstResponder(titleField))
        titleField.stringValue = model.staleTitle
        titleField.delegate?.controlTextDidChange?(
            Notification(
                name: NSControl.textDidChangeNotification,
                object: titleField
            )
        )
        await SnapshotTestSupport.settle(host)

        let store = MappingFixtures.store(mapping: nil)
        ImportSearchFlow.applyMetadata(
            importer: model.importer,
            importStore: store,
            endEditing: {
                await editingCommands.commitActiveEdits()
                window.makeFirstResponder(nil)
            },
            key: MappingFixtures.candidateKey,
            provenance: provenance
        )
        try await waitUntil { model.applicationCount == 1 }

        _ = window.makeFirstResponder(nil)
        await SnapshotTestSupport.settle(host)

        XCTAssertEqual(model.edit.albumTitle, model.appliedTitle)
        XCTAssertEqual(
            model.events,
            [.commit(model.staleTitle), .application]
        )
        window.contentView = nil
        window.orderOut(nil)
    }

    private func waitUntil(_ predicate: () -> Bool) async throws {
        for _ in 0..<100 where !predicate() {
            await Task.yield()
        }
        _ = try XCTUnwrap(predicate() ? true : nil)
    }
}

@MainActor
private final class MetadataApplicationEditingModel {
    enum Event: Equatable {
        case commit(String)
        case application
    }

    let originalTitle = "Original Album Title"
    let staleTitle = "Typed Before Applying"
    let appliedTitle = "Applied Album Title"
    var edit: BridgeRawReleaseEdit
    var events: [Event] = []
    var applicationCount = 0

    init() {
        edit = PreviewData.confirmEditValues
        edit.albumTitle = originalTitle
    }

    var importer: Importer {
        Importer(
            applyCandidateExternalMetadata: { [self] _, _ in
                await apply()
                return 1
            },
            applyCandidateFileTags: { [self] _ in
                await apply()
                return 1
            }
        )
    }

    func commit(field: BridgeCandidateEditField, value: String) {
        guard field == .albumTitle else { return }
        events.append(.commit(value))
        edit.albumTitle = value
    }

    func apply() async {
        events.append(.application)
        applicationCount += 1
        edit.albumTitle = appliedTitle
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
        let importer = Importer(
            subscribeReleaseLibraryStatus: harness.subscribe
        )

        store.refreshLibraryStatusSubscriptions(
            importer: importer,
            key: candidate.key,
            desired: ImportSearchFlow.releaseStatusKeys(state: state())
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
        let candidate = PreviewData.folderCandidates[0]
        store.selectedCandidates[candidate.key] = candidate
        let harness = ReleaseStatusHarness()
        let importer = Importer(
            subscribeReleaseLibraryStatus: harness.subscribe
        )
        let desired = ImportSearchFlow.releaseStatusKeys(state: state())

        store.refreshLibraryStatusSubscriptions(
            importer: importer,
            key: candidate.key,
            desired: desired
        )
        await waitUntil { harness.callbackCount(releaseId: "rel-live") == 1 }

        // The search is cleared and then re-run: the second subscription
        // replaces the first, and only it may write.
        store.refreshLibraryStatusSubscriptions(
            importer: importer,
            key: candidate.key,
            desired: []
        )
        store.refreshLibraryStatusSubscriptions(
            importer: importer,
            key: candidate.key,
            desired: desired
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

    /// A candidate whose typed search turned up one release, as the pane
    /// renders it.
    private func state() -> ImportSearchState {
        PreviewData.searchState(
            identifyState: .idle,
            search: BridgeCandidateSearch(
                query: .general(artist: "Artist Name", album: "Album Title"),
                sources: PreviewData.searchSources(
                    musicbrainz: .done(count: 1),
                    discogs: .notConfigured
                ),
                groups: [
                    BridgeReleaseGroup(
                        id: "group-live",
                        title: "Album Title",
                        artist: "Artist Name",
                        label: nil,
                        coverArt: nil,
                        sources: [
                            BridgeReleaseGroupSource(
                                source: .musicBrainz,
                                groupUrl: "https://example.invalid/group-live"
                            )
                        ],
                        yearMin: 2000,
                        yearMax: 2000,
                        pressings: [
                            BridgePressing(
                                releases: [
                                    BridgeMetadataResult(
                                        source: .musicBrainz,
                                        releaseId: "rel-live",
                                        year: 2000,
                                        format: "CD",
                                        label: nil,
                                        catalogNumber: nil,
                                        country: nil,
                                        barcode: nil,
                                        sourceGroupId: "group-live"
                                    )
                                ],
                                pick: .externalRelease(
                                    source: .musicBrainz,
                                    releaseId: "rel-live",
                                    partners: []
                                )
                            )
                        ]
                    )
                ],
                libraryStatuses: [:],
                status: .found
            )
        )
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
