import AppKit
import BaeKit
import Foundation
import Observation
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
}

@MainActor
@Suite("ImportSearchFlow metadata application")
struct ImportSearchFlowMetadataApplicationTests {
    @Test("the read landing ends the pick; core moves the pane")
    func theReadLandingReturnsToTheDraft() async throws {
        let writes = SessionWriteRecorder()
        let store = unsettledStore(writes: writes)
        let recorder = PickRecorder()
        let importer = Importer(
            applyCandidateExternalMetadata: { _, link, _ in
                await recorder.record(link)
                return .done
            }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            application: .pick(MappingFixtures.link)
        )
        try await Wait.until {
            store.metadataApplicationSession(
                forKey: MappingFixtures.candidateKey
            ) == nil
        }
        #expect(writes.paneMoves(forKey: MappingFixtures.candidateKey).isEmpty)

        #expect(recorder.links == [MappingFixtures.link])
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
            applyCandidateExternalMetadata: { _, link, _ in
                await recorder.record(link)
                for await _ in gate { break }
                return .done
            }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            application: .pick(MappingFixtures.link)
        )
        try await Wait.until {
            recorder.links == [MappingFixtures.link]
        }

        #expect(
            store.loadingPick(forKey: MappingFixtures.candidateKey)?.pressing
                == MappingFixtures.link.record
        )

        releaseGate.finish()
        try await Wait.until {
            store.loadingPick(forKey: MappingFixtures.candidateKey) == nil
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
            applyCandidateExternalMetadata: { _, link, _ in
                await recorder.record(link)
                for await _ in gate { break }
                return .done
            }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            application: .pick(MappingFixtures.link)
        )
        try await Wait.until {
            recorder.links == [MappingFixtures.link]
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
            applyCandidateExternalMetadata: { _, _, _ in
                throw StubError.notImplemented
            }
        )

        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            application: .pick(MappingFixtures.link)
        )
        try await Wait.until {
            store.releaseSelectionFailure(
                forKey: MappingFixtures.candidateKey
            ) != nil
        }

        let after = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(after.releaseLink == nil)
        #expect(after.error == nil)
        #expect(
            store.releaseSelectionFailure(
                forKey: MappingFixtures.candidateKey
            )?
            .release.key == MappingFixtures.releaseId
        )
        #expect(
            writes.reportedFailures().isEmpty
        )
        #expect(
            writes.paneMoves(forKey: MappingFixtures.candidateKey).isEmpty
        )
    }
}

extension ImportSearchFlowMetadataApplicationTests {
    @Test(
        "a release failure whose detail adds to its line keeps the detail",
        arguments: [
            BridgeErrorCategory.internal, .database, .config, .importData,
            .import,
        ]
    )
    func failureDetailAddingToLineIsKept(_ category: BridgeErrorCategory)
        async throws
    {
        let store = unsettledStore()
        let detail = "Downloaded image could not be decoded: unsupported format"
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _, _ in
                throw BridgeError.Diagnostic(category: category, detail: detail)
            }
        )
        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            application: .pick(MappingFixtures.link)
        )
        try await Wait.until {
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
                != nil
        }
        let failure = try #require(
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
        )
        #expect(failure.error.detail == detail)
    }

    /// Core says the tied discs' detail repeats their line, so the row
    /// shows the line alone, without diagnostic controls.
    @Test("a release failure whose line says it all shows no diagnostic")
    func failureLineSayingItAllHasNoDiagnostic() async throws {
        let store = unsettledStore()
        let tied = BridgeErrorCategory.tiedMediums(mediums: [2, 4])
        let importer = Importer(
            applyCandidateExternalMetadata: { _, _, _ in
                throw BridgeError.Diagnostic(
                    category: tied,
                    detail:
                        "discs [2, 4] of the release fit the folder's audio equally"
                )
            }
        )
        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            application: .pick(MappingFixtures.link)
        )
        try await Wait.until {
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
                != nil
        }
        let failure = try #require(
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
        )
        #expect(failure.error.detail == nil)
        #expect(failure.error.line.contains(tied.localizedLine))
    }

    @Test("a replacement release keeps only its own diagnostic")
    func replacementFailureReplacesDiagnostic() async throws {
        let store = unsettledStore()
        for release in ["first-release", "second-release"] {
            let importer = Importer(
                applyCandidateExternalMetadata: { _, _, _ in
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
                application: .pick(
                    BridgePressingLink(
                        record: BridgeLookupRef(
                            catalog: .musicBrainz,
                            key: release
                        ),
                        partners: []
                    )
                )
            )
            try await Wait.until {
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
            applyCandidateExternalMetadata: { _, _, _ in
                throw CancellationError()
            }
        )
        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            application: .pick(MappingFixtures.link)
        )
        try await Wait.until {
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
                metadataProvenance: nil,
                releaseLink: nil
            )
        )
        return store
    }
}

@MainActor
final class MetadataApplicationEditingTests: XCTestCase {
    func testApplyingMetadataReplacesTheFocusedFieldForEverySource()
        async throws
    {
        for application in [
            MetadataApplication.fileTags,
            .pick(
                BridgePressingLink(
                    record: BridgeLookupRef(
                        catalog: .musicBrainz,
                        key: "release-mb"
                    ),
                    partners: []
                )
            ),
            .pick(
                BridgePressingLink(
                    record: BridgeLookupRef(
                        catalog: .discogs,
                        key: "release-discogs"
                    ),
                    partners: []
                )
            ),
        ] {
            try await assertFocusedFieldIsReplaced(by: application)
        }
    }

    private func assertFocusedFieldIsReplaced(
        by application: MetadataApplication
    ) async throws {
        let model = MetadataApplicationEditingModel()
        let editingCommands = EditingCommitCommands()
        let size = NSSize(width: 700, height: 560)
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseMetadataHeader(
                values: model.edit,
                albumArtistStanding: nil,
                writer: ReleaseFieldWriter(
                    setField: { field, value in
                        model.commit(field: field, value: value)
                    }
                ),
                editingCommands: editingCommands,
                artistSearch: { _ in .fixed([], answers: UnansweredCount()) },
                cover: { EmptyView() },
                audioFacts: { EmptyView() },
                albumRow: { EmptyView() }
            )
            .environment(UiStore())
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            host.layoutSubtreeIfNeeded()
            try await SnapshotTestSupport.settle(host)
            host.layoutSubtreeIfNeeded()

            let titleField = try XCTUnwrap(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .first { $0.stringValue == model.originalTitle }
            )
            XCTAssertTrue(HostedInput.focus(titleField, in: window))
            titleField.stringValue = model.staleTitle
            titleField.delegate?.controlTextDidChange?(
                Notification(
                    name: NSControl.textDidChangeNotification,
                    object: titleField
                )
            )
            try await SnapshotTestSupport.settle(host)

            let store = MappingFixtures.store(mapping: nil)
            ImportSearchFlow.applyMetadata(
                importer: model.importer,
                importStore: store,
                endEditing: {
                    await editingCommands.commitActiveEdits()
                    HostedInput.focus(nil, in: window)
                },
                key: MappingFixtures.candidateKey,
                application: application
            )
            try await Wait.until { model.applicationCount == 1 }

            HostedInput.focus(nil, in: window)
            try await SnapshotTestSupport.settle(host)

            XCTAssertEqual(model.edit.albumTitle, model.appliedTitle)
            XCTAssertEqual(
                model.events,
                [.commit(model.staleTitle), .application]
            )
        }
    }
}

@MainActor
@Observable
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
                return .done
            },
            applyCandidateFileMetadata: { [self] _ in
                await apply()
                return .done
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
@Observable
private final class PickRecorder {
    var links: [BridgePressingLink] = []

    func record(_ link: BridgePressingLink) {
        links.append(link)
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
        guard case .drafting(var draft) = detail.stage else {
            Issue.record("the fixture's candidate holds a draft")
            return
        }
        draft.cover = cover
        detail.stage = .drafting(draft: draft)
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
