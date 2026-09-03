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
final class FindOnlinePaneTests: XCTestCase {
    /// Nothing to list means nothing to scroll: the docked form is the whole
    /// of what a folder nobody has looked up yet offers.
    func testAPaneWithNothingToOfferHasNoResultsScroller() async {
        let size = NSSize(width: 900, height: 600)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportSearchPane.preview(state: PreviewData.searchStateIdle)
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
@Suite("Find online verdict")
struct FindOnlineVerdictTests {
    @Test("a folder nobody looked up offers to identify it")
    func idleOffersIdentify() {
        let verdict = FindOnlineVerdict(
            state: .idle,
            toolbar: BridgeSignalsToolbar(signals: [])
        )

        #expect(verdict.lines == [String(localized: "Not identified")])
        #expect(verdict.action == .identify)
        #expect(!verdict.isWorking)
    }

    @Test("a run under way says so and offers nothing")
    func triangulatingWorks() {
        let verdict = FindOnlineVerdict(
            state: .triangulating(discid: .lookingUp, barcode: .scanning),
            toolbar: PreviewData.toolbarBothRunning
        )

        #expect(verdict.isWorking)
        #expect(verdict.action == .none)
    }

    @Test("a verdict names the signals that matched, and only those")
    func foundNamesTheMatchedSignals() {
        let verdict = FindOnlineVerdict(
            state: PreviewData.searchStateFoundExact.identifyState,
            toolbar: PreviewData.toolbarBothMatched
        )

        #expect(verdict.action == .adjust)
        let line = try? #require(verdict.lines.first)
        #expect(line?.contains(String(localized: "Disc ID")) == true)
        #expect(line?.contains(String(localized: "barcode")) == true)
    }

    @Test("an excluded signal is not part of what identified the folder")
    func foundSkipsExcludedSignals() {
        let verdict = FindOnlineVerdict(
            state: PreviewData.searchStateFoundExact.identifyState,
            toolbar: PreviewData.toolbarBarcodeExcluded
        )

        let line = try? #require(verdict.lines.first)
        #expect(line?.contains(String(localized: "Disc ID")) == true)
        #expect(line?.contains(String(localized: "barcode")) == false)
    }

    @Test("a verdict stood back up from the store names no signals")
    func resumedVerdictNamesNoSignals() {
        let verdict = FindOnlineVerdict(
            state: PreviewData.searchStateFoundExact.identifyState,
            toolbar: BridgeSignalsToolbar(signals: [])
        )

        #expect(verdict.lines == [String(localized: "Identified")])
        #expect(verdict.action == .adjust)
    }

    @Test("nothing found names the signals that ran")
    func notFoundNamesTheSignalsThatRan() {
        let verdict = FindOnlineVerdict(
            state: .notFoundAnywhere,
            toolbar: PreviewData.toolbarNothingMatched
        )

        let line = try? #require(verdict.lines.first)
        #expect(line?.contains(String(localized: "Disc ID")) == true)
        #expect(line?.contains(String(localized: "barcode")) == true)
        #expect(verdict.action == .adjust)
    }

    @Test("a folder with no signals has nothing to adjust")
    func manualOnlyOffersNothing() {
        let verdict = FindOnlineVerdict(
            state: .manualOnly(trackCount: 9),
            toolbar: PreviewData.toolbarSkippedNoSignals
        )

        #expect(
            verdict.lines == [String(localized: "No signals in this folder")]
        )
        #expect(verdict.action == .none)
    }

    @Test("a failure names one line per source, with the reason in its help")
    func failureNamesEachSource() {
        let verdict = FindOnlineVerdict(
            state: .failed(
                failures: [
                    .discId(failure: .network),
                    .barcode(source: .discogs, failure: .timeout),
                ],
                groups: [],
                libraryStatuses: [:],
                provenance: [:]
            ),
            toolbar: BridgeSignalsToolbar(signals: [])
        )

        #expect(verdict.isFailure)
        #expect(verdict.action == .retry)
        #expect(verdict.lines.count == 2)
        #expect(
            verdict.lines[0]
                .contains(
                    bridgeMetadataSourceName(source: .musicBrainz)
                )
        )
        #expect(
            verdict.lines[1]
                .contains(
                    bridgeMetadataSourceName(source: .discogs)
                )
        )
        #expect(verdict.help.contains(BridgeLookupFailure.timeout.badgeLine))
    }
}

@MainActor
@Suite("Find online result area")
struct FindOnlineResultAreaTests {
    @Test("a submitted search owns the area whatever identification said")
    func aSearchOwnsTheArea() {
        let found = PreviewData.searchStateFoundExact.identifyState
        #expect(
            FindOnlineResultArea(identifyState: found, hasSearch: true)
                == .searchRun
        )
        #expect(
            FindOnlineResultArea(identifyState: .idle, hasSearch: true)
                == .searchRun
        )
    }

    @Test("each identify state picks its own area")
    func eachStatePicksItsArea() {
        #expect(
            FindOnlineResultArea(identifyState: .idle, hasSearch: false)
                == .notStarted
        )
        #expect(
            FindOnlineResultArea(
                identifyState: .triangulating(
                    discid: .computing,
                    barcode: .scanning
                ),
                hasSearch: false
            ) == .identifying
        )
        #expect(
            FindOnlineResultArea(
                identifyState: PreviewData.searchStateFoundExact.identifyState,
                hasSearch: false
            ) == .groups
        )
        #expect(
            FindOnlineResultArea(
                identifyState: .notFoundAnywhere,
                hasSearch: false
            ) == .nothingFound
        )
        #expect(
            FindOnlineResultArea(
                identifyState: .manualOnly(trackCount: 9),
                hasSearch: false
            ) == .noSignals
        )
    }

    /// One source failing never blanks the pane: the other's matches stand,
    /// and only a run that turned up nothing at all shows the reasons.
    @Test("a failure with matches still lists them")
    func aFailureWithMatchesListsThem() {
        #expect(
            FindOnlineResultArea(
                identifyState:
                    PreviewData.searchStateSourceFailure.identifyState,
                hasSearch: false
            ) == .groups
        )
        #expect(
            FindOnlineResultArea(
                identifyState:
                    PreviewData.searchStateAllSourcesFailed.identifyState,
                hasSearch: false
            ) == .failureLines
        )
    }
}

@MainActor
@Suite("Adjusting a candidate's signals")
struct SignalAdjustPopoverTests {
    /// Clicking the disc ID or the barcode takes it in or out of the run. The
    /// catalog is not a toggle: it is chosen by value, so it sends its own.
    @Test("a signal row's click is the toggle for that signal")
    func aRowSendsItsOwnToggle() {
        func signal(_ kind: BridgeSignalKind) -> BridgeToolbarSignal {
            BridgeToolbarSignal(
                kind: kind,
                value: "value",
                origin: .artwork,
                state: .found(count: 1),
                excluded: false,
                options: []
            )
        }

        #expect(BridgeSignalToggle(signal: signal(.discId)) == .disc)
        #expect(BridgeSignalToggle(signal: signal(.barcode)) == .barcode)
        #expect(BridgeSignalToggle(signal: signal(.catalog)) == nil)
    }

    /// Every signal core extracted gets a control, and Run again closes the
    /// list — including for a verdict resumed from the store, which has no
    /// signals to show and so offers Run again alone.
    @Test("the popover offers one control per signal, plus Run again")
    func thePopoverOffersOneControlPerSignal() async {
        let withSignals = await controlCount(
            SignalAdjustPopover(
                toolbar: PreviewData.toolbarBothMatched,
                onToggle: { _ in },
                onRerun: {},
            )
        )
        let resumed = await controlCount(
            SignalAdjustPopover(
                toolbar: BridgeSignalsToolbar(signals: []),
                onToggle: { _ in },
                onRerun: {},
            )
        )

        #expect(
            withSignals
                == PreviewData.toolbarBothMatched.signals.count + 1
        )
        #expect(resumed == 1)
    }

    private func controlCount<V: View>(_ view: V) async -> Int {
        let size = NSSize(width: 340, height: 220)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            view.frame(width: size.width, height: size.height),
            size: size
        )
        await SnapshotTestSupport.settle(host)
        let count = SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSButton }
            .count
        withExtendedLifetime(window) {}
        return count
    }
}

@MainActor
@Suite("ImportSearchFlow metadata application")
struct ImportSearchFlowMetadataApplicationTests {
    @Test("command return does not confirm until detail delivery")
    func commandReturnDoesNotConfirmBeforeDetailDelivery() async throws {
        let store = unsettledStore()
        let confirmation = ConfirmationRecorder()
        let recorder = PickRecorder()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, source, releaseId in
                await recorder.record(
                    .externalRelease(source: source, releaseId: releaseId)
                )
                return 1
            }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: MappingFixtures.provenance,
            onConfirmed: confirmation.record
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataApplicationSession?
                .commandRevision == 1
        }

        #expect(recorder.provenances == [MappingFixtures.provenance])
        #expect(confirmation.count == 0)
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
        #expect(confirmation.count == 1)
    }

    @Test("detail delivery before command return still waits for both")
    func detailDeliveryBeforeCommandReturnWaitsForBoth() async throws {
        let store = unsettledStore()
        let confirmation = ConfirmationRecorder()
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

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: MappingFixtures.provenance,
            onConfirmed: confirmation.record
        )
        await waitUntil {
            recorder.provenances == [MappingFixtures.provenance]
        }

        deliverPickedDetail(to: store)

        #expect(confirmation.count == 0)
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .provenanceInFlight == MappingFixtures.provenance
        )

        releaseGate.finish()
        await waitUntil { confirmation.count == 1 }
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .provenanceInFlight == nil
        )
    }

    @Test("a different detail cannot confirm the chosen pressing")
    func mismatchedDetailCannotConfirmTheChoice() async throws {
        let store = unsettledStore()
        let confirmation = ConfirmationRecorder()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _, _ in 1 }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: MappingFixtures.provenance,
            onConfirmed: confirmation.record
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

        #expect(confirmation.count == 0)
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .provenanceInFlight == MappingFixtures.provenance
        )

        deliverPickedDetail(to: store)
        #expect(confirmation.count == 1)
    }

    @Test(
        "an older revision from the same release cannot confirm reapplication"
    )
    func olderSameReleaseRevisionCannotConfirm() async throws {
        let store = unsettledStore()
        let confirmation = ConfirmationRecorder()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _, _ in 2 }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: MappingFixtures.provenance,
            onConfirmed: confirmation.record
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataApplicationSession?
                .commandRevision == 2
        }

        deliverPickedDetail(to: store, revision: 1)
        #expect(confirmation.count == 0)

        deliverPickedDetail(to: store, revision: 2)
        #expect(confirmation.count == 1)
    }

    @Test("a failed choice does not confirm and shows the error")
    func failedChoiceDoesNotConfirmAndShowsError() async throws {
        let store = unsettledStore()
        let confirmation = ConfirmationRecorder()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _, _ in
                throw StubError.notImplemented
            }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: MappingFixtures.provenance,
            onConfirmed: confirmation.record
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
        #expect(confirmation.count == 0)
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
final class MetadataApplicationEditingTests: XCTestCase {
    func testApplyingMetadataReplacesTheFocusedFieldForEverySource()
        async throws
    {
        for provenance in [
            BridgeMetadataProvenance.fileTags,
            .externalRelease(source: .musicBrainz, releaseId: "release-mb"),
            .externalRelease(source: .discogs, releaseId: "release-discogs"),
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
            applyCandidateExternalMetadata: { [self] _, _, _ in
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
private final class ConfirmationRecorder {
    private(set) var count = 0

    func record() {
        count += 1
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
                musicbrainz: .done(count: 1),
                discogs: .notConfigured,
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
                            BridgePressing(releases: [
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
                            ])
                        ]
                    )
                ],
                libraryStatuses: [:],
                settled: true
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
