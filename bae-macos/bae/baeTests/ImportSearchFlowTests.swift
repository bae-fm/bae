import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing
import XCTest

@testable import bae

@MainActor
final class SettingsNavigationTests: XCTestCase {
    /// The Discogs key lives under the source switch it unlocks, so the "open
    /// settings" affordance on the Find online bar lands on the Import pane.
    func testOpeningTheDiscogsKeySelectsTheImportPaneBeforePresentation() {
        let navigation = SettingsNavigation()
        var selectionAtPresentation: SettingsTab?

        navigation.open(.importing) {
            selectionAtPresentation = navigation.selectedTab
        }

        #expect(selectionAtPresentation == .importing)
    }

    func testDiscogsKeyFieldTakesFocusWhenItAppears() async {
        let size = NSSize(width: 500, height: 320)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            Form {
                Section {
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
                }
            }
            .formStyle(.grouped)
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
            .release.key == MappingFixtures.releaseId
        )
        #expect(
            !writes.errors(forKey: MappingFixtures.candidateKey)
                .contains { $0 != nil }
        )
        #expect(
            writes.presentations(forKey: MappingFixtures.candidateKey).isEmpty
        )
    }

}

extension ImportSearchFlowMetadataApplicationTests {
    @Test(
        "unexpected release failures retain their diagnostic",
        arguments: [
            BridgeErrorCategory.internal, .database, .config, .importData,
        ]
    )
    func unexpectedFailureRetainsDiagnostic(_ category: BridgeErrorCategory)
        async throws
    {
        let store = unsettledStore()
        let detail = "Downloaded image could not be decoded: unsupported format"
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _ in
                throw BridgeError.Diagnostic(category: category, detail: detail)
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
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
                != nil
        }
        let failure = try #require(
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
        )
        #expect(failure.error.detail == detail)
    }

    @Test(
        "expected release failures keep their friendly line without diagnostic controls"
    )
    func expectedFailureHasNoDiagnostic() async throws {
        let store = unsettledStore()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _ in
                throw BridgeError.Diagnostic(
                    category: .import,
                    detail: "Provider returned 500"
                )
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
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
                != nil
        }
        let failure = try #require(
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
        )
        #expect(failure.error.detail == nil)
        #expect(
            failure.error.line.contains(
                BridgeErrorCategory.import.localizedLine
            )
        )
    }

    @Test("a replacement release keeps only its own diagnostic")
    func replacementFailureReplacesDiagnostic() async throws {
        let store = unsettledStore()
        for release in ["first-release", "second-release"] {
            let importer = Importer(
                applyCandidateExternalMetadata: { _, _ in
                    throw BridgeError.Diagnostic(
                        category: .database,
                        detail: "Failure for \(release)"
                    )
                }
            )
            ImportSearchFlow.applyMetadata(
                importer: importer,
                importStore: store,
                endEditing: {},
                key: MappingFixtures.candidateKey,
                provenance: .externalRelease(
                    record: BridgeMetadataRef(
                        catalog: .musicBrainz,
                        key: release
                    ),
                    partners: []
                )
            )
            await waitUntil {
                store.releaseSelectionFailure(
                    forKey: MappingFixtures.candidateKey
                )?
                .release.key == release
            }
            let failure = try #require(
                store.releaseSelectionFailure(
                    forKey: MappingFixtures.candidateKey
                )
            )
            #expect(failure.error.detail == "Failure for \(release)")
        }
    }

    @Test("cancelling a release read leaves no failure")
    func cancelledReadHasNoFailure() async throws {
        let store = unsettledStore()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _ in throw CancellationError()
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
            store.metadataApplicationSession(
                forKey: MappingFixtures.candidateKey
            ) == nil
        }
        #expect(
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
                == nil
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
            BridgeMetadataProvenance.fileMetadata,
            .externalRelease(
                record: BridgeMetadataRef(
                    catalog: .musicBrainz,
                    key: "release-mb"
                ),
                partners: []
            ),
            .externalRelease(
                record: BridgeMetadataRef(
                    catalog: .discogs,
                    key: "release-discogs"
                ),
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
                audioFacts: { EmptyView() }
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
            applyCandidateFileMetadata: { [self] _ in
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
        "a pane's offered releases stay live through one read until the candidate closes"
    )
    func resultStatusUpdatesAndCancels() async throws {
        let store = ImportStore()
        let candidate = PreviewData.folderCandidates[0]
        store.selectedCandidates[candidate.key] = candidate
        let feed = LibraryStatusFeed()
        let importer = Importer(subscribeLibraryStatuses: { feed.query() })

        store.refreshLibraryStatusSubscriptions(
            importer: importer,
            key: candidate.key,
            desired: ImportSearchFlow.releaseStatusKeys(state: state())
        )
        #expect(feed.requested.last?.map(\.releaseId) == ["rel-live"])

        feed.deliver(revision: 1, status("album-live"))
        await waitUntil {
            store.candidate(forKey: candidate.key)?
                .libraryStatuses["rel-live"]?
                .albumId == "album-live"
        }

        store.selectedCandidates.removeValue(forKey: candidate.key)
        await waitUntil { feed.cancelled }
    }

    @Test(
        "new offers move the same read, and a value for replaced ones is not shown"
    )
    func offersMoveOneRead() async throws {
        let store = ImportStore()
        let candidate = PreviewData.folderCandidates[0]
        store.selectedCandidates[candidate.key] = candidate
        let feed = LibraryStatusFeed()
        let importer = Importer(subscribeLibraryStatuses: { feed.query() })
        let desired = ImportSearchFlow.releaseStatusKeys(state: state())

        // The search is offered, cleared, and then re-run.
        for checks in [desired, [], desired] {
            store.refreshLibraryStatusSubscriptions(
                importer: importer,
                key: candidate.key,
                desired: checks
            )
        }
        #expect(feed.opened == 1)
        #expect(feed.requested.count == 3)

        feed.deliver(revision: 1, status("album-old"))
        for _ in 0..<50 { await Task.yield() }
        #expect(
            store.candidate(forKey: candidate.key)?
                .libraryStatuses["rel-live"] == nil
        )

        feed.deliver(revision: 3, status("album-new"))
        await waitUntil {
            store.candidate(forKey: candidate.key)?
                .libraryStatuses["rel-live"]?
                .albumId == "album-new"
        }
    }

    private func status(_ albumId: String) -> BridgeLibraryStatus {
        BridgeLibraryStatus(
            releaseId: "rel-live",
            releaseInLibrary: true,
            albumInLibrary: true,
            albumTitle: "Album Title",
            albumId: albumId
        )
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
                                groupUrl: "https://example.invalid/group-live",
                                albumLinksUnread: false
                            )
                        ],
                        yearMin: 2000,
                        yearMax: 2000,
                        sections: [
                            BridgePressingSection(
                                album: nil,
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
                                                barcodes: [],
                                                sourceGroupId: "group-live"
                                            )
                                        ],
                                        pick: .externalRelease(
                                            record: BridgeMetadataRef(
                                                catalog: .musicBrainz,
                                                key: "rel-live"
                                            ),
                                            partners: []
                                        )
                                    )
                                ],
                                narrowedOut: []
                            )
                        ]
                    )
                ],
                libraryStatuses: [:],
                status: .found
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

/// A library-status read the test drives: it records every check request,
/// numbers each one as core would, and hands `next` whatever the test
/// delivers.
private final class LibraryStatusFeed: @unchecked Sendable {
    private let lock = NSLock()
    private var openedCount = 0
    private var requests: [[BridgeLibraryCheck]] = []
    private var pending: [BridgeLibraryStatusSnapshot] = []
    private var waiter:
        CheckedContinuation<BridgeLibraryStatusSnapshot, any Error>?
    private var wasCancelled = false

    var opened: Int { lock.withLock { openedCount } }
    var requested: [[BridgeLibraryCheck]] { lock.withLock { requests } }
    var cancelled: Bool { lock.withLock { wasCancelled } }

    func query() -> LibraryStatusQuery {
        lock.withLock { openedCount += 1 }
        return LibraryStatusQuery(
            setChecks: { [self] checks in
                lock.withLock {
                    requests.append(checks)
                    return UInt64(requests.count)
                }
            },
            next: { [self] in
                try await withCheckedThrowingContinuation { continuation in
                    let ready: BridgeLibraryStatusSnapshot? = lock.withLock {
                        if pending.isEmpty {
                            waiter = continuation
                            return nil
                        }
                        return pending.removeFirst()
                    }
                    if let ready { continuation.resume(returning: ready) }
                }
            },
            cancel: { [self] in
                lock.withLock { wasCancelled = true }
            }
        )
    }

    func deliver(revision: UInt64, _ status: BridgeLibraryStatus) {
        let snapshot = BridgeLibraryStatusSnapshot(
            statuses: [status.releaseId: status],
            requestRevision: revision
        )
        let waiter:
            CheckedContinuation<BridgeLibraryStatusSnapshot, any Error>? =
                lock.withLock {
                    if let waiter = self.waiter {
                        self.waiter = nil
                        return waiter
                    }
                    pending.append(snapshot)
                    return nil
                }
        waiter?.resume(returning: snapshot)
    }
}
