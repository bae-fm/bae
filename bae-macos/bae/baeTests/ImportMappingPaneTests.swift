import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Recording stubs for the services the mapping table's controls reach.
///
/// Every control writes: nothing here answers with a table, because the pane
/// has none to replace. What each recorder holds is the call core would have
/// received, and the next value of the candidate is what would redraw it.
@MainActor
private final class Recorder {
    struct RoleCall {
        let key: String
        let fileId: String
        let choice: BridgeFileRoleChoice
    }

    var roleCalls: [RoleCall] = []
    var bindCalls: [(sheetFileId: String, audioFileId: String?)] = []
    var discCalls: [(sheetFileId: String, disc: BridgeSheetDisc)] = []
    var trackEdits: [(key: String, track: BridgeRawTrackEdit)] = []
    var droppedTracks: [(key: String, trackId: String)] = []
    var editFields: [(field: BridgeCandidateEditField, value: String)] = []
    var externalMetadata: [(source: BridgeMetadataSource, releaseId: String)] =
        []
    var fileTagsApplications = 0
    var played: [BridgePreviewTarget] = []
    var stops = 0
    /// What the pick call throws, when the test is about a pick that fails.
    var pickFailure: (any Error)?

    var importer: Importer {
        Importer(
            setSheetBinding: { [self] _, sheetFileId, audioFileId in
                await MainActor.run {
                    bindCalls.append(
                        (sheetFileId: sheetFileId, audioFileId: audioFileId)
                    )
                }
            },
            applyCandidateExternalMetadata: {
                [self] _, source, releaseId in
                try await MainActor.run {
                    externalMetadata.append(
                        (source: source, releaseId: releaseId)
                    )
                    if let pickFailure { throw pickFailure }
                    return 1
                }
            },
            applyCandidateFileTags: { [self] _ in
                try await MainActor.run {
                    fileTagsApplications += 1
                    if let pickFailure { throw pickFailure }
                    return 1
                }
            },
            setSheetDisc: { [self] _, sheetFileId, disc in
                await MainActor.run {
                    discCalls.append((sheetFileId: sheetFileId, disc: disc))
                }
            },
            setFileRole: { [self] key, fileId, choice in
                await MainActor.run {
                    roleCalls.append(
                        RoleCall(key: key, fileId: fileId, choice: choice)
                    )
                }
            },
            setCandidateEditField: { [self] _, field, value in
                await MainActor.run {
                    editFields.append((field: field, value: value))
                }
            },
            setCandidateTrackEdit: { [self] key, track in
                await MainActor.run {
                    trackEdits.append((key: key, track: track))
                }
            },
            dropCandidateTrack: { [self] key, trackId in
                await MainActor.run {
                    droppedTracks.append((key: key, trackId: trackId))
                }
            }
        )
    }

    var previewAudio: PreviewAudio {
        PreviewAudio(
            previewPlay: { [self] target in
                MainActor.assumeIsolated { played.append(target) }
            },
            previewStop: { [self] in
                MainActor.assumeIsolated { stops += 1 }
            }
        )
    }

    func services(_ store: ImportStore) -> ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            identifyAutomatically: true,
            importStore: store,
            endEditing: {},
            previewAudio: previewAudio,
            openDocument: { _, _ in },
            openImages: { _, _ in },
            onError: { _ in },
        )
    }
}

@MainActor
private func captureMappingPane(
    candidate: Candidate,
    runtime: BridgeCandidateRuntimeSnapshot?
) async throws -> Data {
    let size = NSSize(width: 1200, height: 760)
    let (window, host) = SnapshotTestSupport.hostInWindow(
        ImportMappingPreview.make(
            candidate: candidate,
            storageCloud: .constant(false),
            storagePinned: .constant(false),
            runtime: runtime
        )
        .frame(width: size.width, height: size.height)
        .importPreviewEnvironment()
        .candidateReaderPreviewEnvironment(),
        size: size
    )
    await SnapshotTestSupport.settle(host)
    let capture = try await SnapshotTestSupport.capturePNG(host, size: size)
    withExtendedLifetime(window) {}
    return capture
}

private func runtime(
    _ identifyState: BridgeIdentifyState
) -> BridgeCandidateRuntimeSnapshot {
    BridgeCandidateRuntimeSnapshot(
        identifyState: identifyState,
        signalsToolbar: BridgeSignalsToolbar(signals: []),
        import: nil,
        search: nil
    )
}

@Suite("Import mapping pane")
struct ImportMappingPaneTests {
    @MainActor
    @Test("candidate mapping remains visible before metadata is affirmed")
    func unseededCandidateKeepsMappingVisible() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable,
            metadataProvenance: nil,
            edit: MappingFixtures.blankEdit
        )
        let candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        let size = NSSize(width: 1_000, height: 760)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportMappingPreview.make(
                candidate: candidate,
                storageCloud: .constant(false),
                storagePinned: .constant(false)
            )
            .frame(width: size.width, height: size.height)
            .importPreviewEnvironment(),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let scrollViews = SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSScrollView }
        // The pane itself scrolls vertically; the mapping table adds its own
        // horizontal scroller for the candidate's source-to-track rows.
        #expect(scrollViews.count >= 2)
        withExtendedLifetime(window) {}
    }

    @MainActor
    @Test(
        "restored and arriving match lists appear inline until identity settles"
    )
    func matchListsAppearInlineUntilIdentitySettles() async throws {
        var browsing = Candidate(
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.thirteenFileTable,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil
            )
        )
        browsing.metadataPresentation = .findOnline
        browsing.resumedIdentifyState = IdentifyState(
            bridge: PreviewData.bridgeDisagreementState
        )
        let restoredMatches = try await captureMappingPane(
            candidate: browsing,
            runtime: nil
        )

        browsing.resumedIdentifyState = .manualOnly(trackCount: 11)
        let before = try await captureMappingPane(
            candidate: browsing,
            runtime: nil
        )
        #expect(restoredMatches != before)

        let matches = try await captureMappingPane(
            candidate: browsing,
            runtime: runtime(PreviewData.bridgeDisagreementState)
        )
        #expect(matches != before)

        // A stored pick settles the identity: the match list gives way to the
        // release card, whatever the run left behind.
        let settled = Candidate(
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.thirteenFileTable
            )
        )
        let settledConflict = try await captureMappingPane(
            candidate: settled,
            runtime: runtime(PreviewData.bridgeDisagreementState)
        )
        let settledWithoutConflict = try await captureMappingPane(
            candidate: settled,
            runtime: runtime(.manualOnly(trackCount: 11))
        )
        #expect(settledConflict.elementsEqual(settledWithoutConflict))
    }

    // 1. Binding a track sheet is a decision about the folder, so it goes to
    //    core and nothing else. The table it re-shapes — one container into the
    //    release's twelve entries — arrives as the candidate's next value.
    @MainActor
    @Test("binding a sheet is written and nothing else")
    func bindingASheetIsWritten() async {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.unboundSheetTable
        )
        #expect(MappingFixtures.mapping(of: store).willWriteCount == 1)
        #expect(
            MappingFixtures.mapping(of: store).reconciliation
                == .moreTracks(files: 1, tracks: 12)
        )

        let recorder = Recorder()
        await ImportMappingFlow.bindSheet(
            key: MappingFixtures.candidateKey,
            sheetFileId: MappingFixtures.sheetId,
            audioFileId: MappingFixtures.containerId,
            services: recorder.services(store)
        )

        #expect(recorder.bindCalls.count == 1)
        #expect(
            recorder.bindCalls.first?.sheetFileId == MappingFixtures.sheetId
        )
        #expect(
            recorder.bindCalls.first?.audioFileId
                == MappingFixtures.containerId
        )
        // The pane does not rewrite its own table: it still shows what it was
        // handed, and the next read replaces it whole.
        #expect(MappingFixtures.mapping(of: store).willWriteCount == 1)

        // What that read comes back with is the sheet's group.
        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.boundSheetTable()
            )
        )
        let after = MappingFixtures.mapping(of: store)
        #expect(after.trackSections.count == 1)
        guard
            case .sheet(let sheet, let entries) = after.trackSections[0].content
        else {
            Issue.record(
                "expected a sheet section, got \(after.trackSections[0])"
            )
            return
        }
        #expect(sheet.bound.containerId == MappingFixtures.containerId)
        #expect(entries.count == 12)
        #expect(after.willWriteCount == 12)
        #expect(after.reconciliation == .agrees(count: 12))
    }

    // 2. Naming a row writes that row. The count the bar states comes from the
    //    table core answers with, so the write is the whole of what the field
    //    does.
    @MainActor
    @Test("naming an unmatched row writes the row")
    func namingAnUnmatchedRowWritesTheRow() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        #expect(MappingFixtures.mapping(of: store).willWriteCount == 13)
        #expect(MappingFixtures.mapping(of: store).unansweredCount == 1)

        let unnamed = try #require(
            MappingFixtures.mapping(of: store).trackMappings.last?.track
        )
        var named = unnamed
        named.title = "Hidden Track"
        let recorder = Recorder()
        await ImportMappingFlow.editTrack(
            key: MappingFixtures.candidateKey,
            track: named,
            services: recorder.services(store)
        )

        #expect(recorder.trackEdits.count == 1)
        #expect(recorder.trackEdits.first?.key == MappingFixtures.candidateKey)
        #expect(recorder.trackEdits.first?.track.title == "Hidden Track")
        #expect(recorder.trackEdits.first?.track.id == unnamed.id)

        // And the next read is what answers the bar.
        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.thirteenFileTable(
                    lastTitle: "Hidden Track"
                )
            )
        )
        #expect(MappingFixtures.mapping(of: store).willWriteCount == 13)
        #expect(MappingFixtures.mapping(of: store).unansweredCount == 0)
    }

    // 2b. Pointing a row at a different file writes the whole row with its new
    //     audio — a binding is part of the row, not a second thing to store.
    @MainActor
    @Test("pointing a row at a file writes the row with that audio")
    func choosingAFileWritesTheRow() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = Recorder()
        let target = try #require(
            MappingFixtures.mapping(of: store).trackMappings.first?.track
        )

        await ImportMappingFlow.chooseFile(
            key: MappingFixtures.candidateKey,
            trackId: target.id,
            audio: .standalone(fileId: "13.flac"),
            services: recorder.services(store)
        )

        #expect(recorder.trackEdits.count == 1)
        #expect(recorder.trackEdits.first?.track.id == target.id)
        #expect(
            recorder.trackEdits.first?.track.file
                == .standalone(fileId: "13.flac")
        )
    }

    // 2c. Dropping a row takes it out of the import and nothing on disk
    //     changes, so the whole of it is one write core keys by the row.
    @MainActor
    @Test("dropping a row writes the drop")
    func droppingARowWritesTheDrop() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = Recorder()
        let target = try #require(
            MappingFixtures.mapping(of: store).trackMappings.last?.track
        )

        await ImportMappingFlow.drop(
            key: MappingFixtures.candidateKey,
            trackId: target.id,
            services: recorder.services(store)
        )

        #expect(
            recorder.droppedTracks.map(\.trackId) == [target.id]
        )
        #expect(recorder.trackEdits.isEmpty)
    }
}

extension ImportMappingPaneTests {

    // 3. Excluding a file is one write: the role. Its rows leave because the
    //    folder they described is a different set now, which is core's answer
    //    and not the pane's edit.
    @MainActor
    @Test("excluding a file writes only its role")
    func excludingAFileWritesOnlyItsRole() async {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = Recorder()
        let actions = ImportMappingFlow.actions(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )

        actions.exclude("13.flac")
        try? await Task.sleep(for: .milliseconds(50))

        #expect(recorder.roleCalls.count == 1)
        #expect(recorder.roleCalls.first?.fileId == "13.flac")
        #expect(recorder.roleCalls.first?.choice == .notATrack)
        #expect(recorder.trackEdits.isEmpty)
        #expect(recorder.droppedTracks.isEmpty)
    }

    // 4. Nothing in the pane disables the commit. A row nobody named and a
    //    track with no audio behind it are both states core shapes into a
    //    committable edit — the button that used to grey out has no condition
    //    left to grey out on.
    @MainActor
    @Test("nothing disables the commit")
    func nothingDisablesTheCommit() {
        let unanswered = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        #expect(MappingFixtures.isCommittable(unanswered))
        #expect(
            bridgeMappingTracks(table: MappingFixtures.mapping(of: unanswered))
                .count == 13
        )

        // A release that names more tracks than the folder has anything for.
        let unbacked = MappingFixtures.store(
            mapping: MappingFixtures.unboundSheetTable
        )
        #expect(MappingFixtures.isCommittable(unbacked))
        #expect(MappingFixtures.mapping(of: unbacked).willWriteCount == 1)
        #expect(
            bridgeMappingTracks(table: MappingFixtures.mapping(of: unbacked))
                .count == 12
        )
    }

    // 4b. A pick whose fetch drops stores nothing, so the pane keeps showing
    //     what it had and says the command failed. Nothing about the candidate
    //     is rolled back — there was nothing to roll back.
    @MainActor
    @Test("a failed pick leaves the pane as it was and states the failure")
    func aFailedPickLeavesThePaneAsItWas() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let before = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]?.detail
        )
        let recorder = Recorder()
        recorder.pickFailure = StubError.notImplemented

        ImportSearchFlow.applyMetadata(
            importer: recorder.importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: .externalRelease(
                source: MappingFixtures.source,
                releaseId: "another-pressing"
            )
        )
        try await Task.sleep(for: .milliseconds(50))

        let candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.error != nil)
        #expect(candidate.provenanceInFlight == nil)
        #expect(candidate.detail == before)
        #expect(candidate.metadataProvenance == MappingFixtures.provenance)
    }

    // 4c. Typing in a release field writes that one field, once, as the field
    //     is left. Nothing else about the form moves.
    @MainActor
    @Test("leaving release fields writes each distinct year")
    func leavingReleaseFieldsWritesEachDistinctYear() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = Recorder()
        let writer = ReleaseFieldWriter { field, value in
            Task { @MainActor in
                try? await recorder.importer.setCandidateEditField(
                    MappingFixtures.candidateKey,
                    field,
                    value
                )
            }
        }

        await writer.setField(.albumYear, "1987")
        await writer.setField(.pressingYear, "2011")
        try await Task.sleep(for: .milliseconds(50))

        #expect(recorder.editFields.count == 2)
        #expect(
            recorder.editFields.contains {
                $0.field == .albumYear && $0.value == "1987"
            }
        )
        #expect(
            recorder.editFields.contains {
                $0.field == .pressingYear && $0.value == "2011"
            }
        )
        // The store holds no copy of the form: it still reads what core last
        // answered with.
        #expect(
            store.selectedCandidates[MappingFixtures.candidateKey]?
                .edit?
                .pressing.year == MappingFixtures.albumEdit.pressing.year
        )
    }

    // 5. The row's play control auditions that row's own audio. For a sheet
    //    entry that is the container the entry is carved from, which is the
    //    only file on disk there is to play.
    @MainActor
    @Test("auditioning a row plays its own audio")
    func auditioningARowPlaysItsAudio() throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.boundSheetTable()
        )
        let recorder = Recorder()
        let actions = ImportMappingFlow.actions(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        let entry = try #require(
            MappingFixtures.mapping(of: store).trackMappings[4].source
                .previewTarget
        )

        actions.preview(entry)
        #expect(recorder.played == [entry])
        #expect(entry.path == MappingFixtures.containerPath)

        actions.stopPreview()
        #expect(recorder.stops == 1)
    }

    // 6. Cue filenames are arbitrary, so which disc a sheet is is the user's to
    //    say. Saying it is one write; the tracklist it re-shapes arrives as the
    //    candidate's next value.
    @MainActor
    @Test("assigning a cue to a disc is written")
    func assigningACueToADisc() async {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.boundSheetTable()
        )
        let recorder = Recorder()
        await ImportMappingFlow.setSheetDisc(
            key: MappingFixtures.candidateKey,
            sheetFileId: MappingFixtures.sheetId,
            disc: .disc(number: 2),
            services: recorder.services(store)
        )

        #expect(recorder.discCalls.count == 1)
        #expect(recorder.discCalls.first?.disc == .disc(number: 2))

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.boundSheetTable(
                    assignment: .disc(number: 2)
                )
            )
        )
        guard
            case .sheet(let sheet, _) = MappingFixtures.mapping(of: store)
                .trackSections[0].content
        else {
            Issue.record("expected a sheet row")
            return
        }
        #expect(sheet.assignment == .disc(number: 2))
    }

    // 7. An ignored cue contributes nothing to the tracklist, and its container
    //    is loose audio again.
    @MainActor
    @Test("ignoring a cue takes its entries out of the tracklist")
    func ignoringACue() async {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.boundSheetTable()
        )
        let recorder = Recorder()
        await ImportMappingFlow.setSheetDisc(
            key: MappingFixtures.candidateKey,
            sheetFileId: MappingFixtures.sheetId,
            disc: .ignored,
            services: recorder.services(store)
        )

        #expect(recorder.discCalls.first?.disc == .ignored)

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.ignoredSheetTable
            )
        )
        let mapping = MappingFixtures.mapping(of: store)
        guard case .sheet(let sheet) = mapping.files[0] else {
            Issue.record("expected an unassociated sheet file")
            return
        }
        #expect(sheet.assignment == .ignored)
        #expect(mapping.willWriteCount == 1)
    }

    // 8. Applying each source writes that source. Browsing either source does
    //    not replace the draft and is covered separately.
    @MainActor
    @Test("applying File Tags and an online release writes both sources")
    func applyingMetadataSourcesWritesBothSources() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = Recorder()

        ImportSearchFlow.applyMetadata(
            importer: recorder.importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: .fileTags
        )
        try await Task.sleep(for: .milliseconds(50))
        #expect(recorder.fileTagsApplications == 1)

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.fileTagsTable,
                metadataProvenance: .fileTags
            )
        )
        var candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.metadataProvenance == .fileTags)
        #expect(candidate.mapping.trackMappings.count == 2)
        #expect(candidate.mapping.reconciliation == nil)
        #expect(candidate.pickedRelease == nil)

        ImportSearchFlow.applyMetadata(
            importer: recorder.importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: MappingFixtures.provenance
        )
        try await Task.sleep(for: .milliseconds(50))
        #expect(
            recorder.externalMetadata.map(\.releaseId)
                == [MappingFixtures.releaseId]
        )

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.thirteenFileTable
            )
        )
        candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.metadataProvenance == MappingFixtures.provenance)
        #expect(candidate.mapping.trackMappings.count == 13)
        #expect(candidate.mapping.willWriteCount == 13)
        #expect(
            candidate.pickedRelease?.releaseId == MappingFixtures.releaseId
        )
    }
}
