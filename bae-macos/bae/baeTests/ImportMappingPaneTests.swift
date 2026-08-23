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
    var roleCalls:
        [(key: String, fileId: String, choice: BridgeFileRoleChoice)] = []
    var bindCalls: [(sheetFileId: String, audioFileId: String?)] = []
    var discCalls: [(sheetFileId: String, disc: BridgeSheetDisc)] = []
    var trackEdits: [(key: String, track: BridgeRawTrackEdit)] = []
    var droppedTracks: [(key: String, trackId: String)] = []
    var editFields: [(field: BridgeCandidateEditField, value: String)] = []
    var picks: [BridgeIdentityPick] = []
    var played: [String] = []
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
            pickCandidateIdentity: { [self] _, pick in
                try await MainActor.run {
                    picks.append(pick)
                    if let pickFailure { throw pickFailure }
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
                        (key: key, fileId: fileId, choice: choice)
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
            previewPlay: { [self] path in
                MainActor.assumeIsolated { played.append(path) }
            },
            previewStop: { [self] in
                MainActor.assumeIsolated { stops += 1 }
            }
        )
    }

    func services(_ store: ImportStore) -> ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            importStore: store,
            previewAudio: previewAudio,
            openDocument: { _, _ in },
            openImages: { _, _ in },
            onError: { _ in },
        )
    }
}

@Suite("Import mapping pane")
struct ImportMappingPaneTests {
    @MainActor
    @Test(
        "restored and arriving conflicts appear inline until identity settles"
    )
    func conflictsAppearInlineUntilIdentitySettles() async throws {
        let scene = PreviewData.importTabScene()
        let store = scene.store
        let key = PreviewData.importTabConflictCandidate.key
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        uiStore.setFolderCandidateSelection([key])
        let size = NSSize(width: 1200, height: 760)
        let (_, host) = SnapshotTestSupport.hostInWindow(
            ImportView()
                .environment(uiStore)
                .importPreviewEnvironment()
                .environment(uiStore)
                .environment(scene.slot(uiStore: uiStore))
                .environment(Library.stub())
                .environment(PreviewAudio.stub())
                .environment(store)
                .environment(PreviewData.importTabImporter())
                .environment(
                    \.candidateRuntimePublisher,
                    store.candidateRuntimeSubject.eraseToAnyPublisher()
                ),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let restoredConflict = try await SnapshotTestSupport.capturePNG(
            host,
            size: size
        )

        store.candidateRuntimeSubject.send(
            .updated(
                key: key,
                runtime: BridgeCandidateRuntimeSnapshot(
                    identifyState: .manualOnly(trackCount: 11),
                    signalsToolbar: BridgeSignalsToolbar(signals: []),
                    import: nil
                )
            )
        )
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let before = try await SnapshotTestSupport.capturePNG(host, size: size)

        #expect(restoredConflict != before)

        store.candidateRuntimeSubject.send(
            .updated(
                key: key,
                runtime: BridgeCandidateRuntimeSnapshot(
                    identifyState: PreviewData.bridgeConflictState,
                    signalsToolbar: BridgeSignalsToolbar(signals: []),
                    import: nil
                )
            )
        )
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let conflict = try await SnapshotTestSupport.capturePNG(
            host,
            size: size
        )

        #expect(conflict != before)

        // A stored pick settles the identity: the conflict offer gives way to
        // the release card, whatever the run left behind.
        store.applyCandidateDetail(
            key: key,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.thirteenFileTable
            )
        )
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let settledConflict = try await SnapshotTestSupport.capturePNG(
            host,
            size: size
        )

        store.candidateRuntimeSubject.send(
            .updated(
                key: key,
                runtime: BridgeCandidateRuntimeSnapshot(
                    identifyState: .manualOnly(trackCount: 11),
                    signalsToolbar: BridgeSignalsToolbar(signals: []),
                    import: nil
                )
            )
        )
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let settledWithoutConflict = try await SnapshotTestSupport.capturePNG(
            host,
            size: size
        )

        #expect(settledConflict == settledWithoutConflict)
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
        #expect(after.rows.count == 1)
        guard case .sheet(let sheet, let entries) = after.rows[0] else {
            Issue.record("expected a sheet row, got \(after.rows[0])")
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
            MappingFixtures.mapping(of: store).units.last?.track
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
            MappingFixtures.mapping(of: store).units.first?.track
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
            MappingFixtures.mapping(of: store).units.last?.track
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
            bridgeMappingTracks(table: MappingFixtures.mapping(of: unanswered)).count == 13
        )

        // A release that names more tracks than the folder has anything for.
        let unbacked = MappingFixtures.store(
            mapping: MappingFixtures.unboundSheetTable
        )
        #expect(MappingFixtures.isCommittable(unbacked))
        #expect(MappingFixtures.mapping(of: unbacked).willWriteCount == 1)
        #expect(bridgeMappingTracks(table: MappingFixtures.mapping(of: unbacked)).count == 12)
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

        ImportSearchFlow.decideIdentity(
            importer: recorder.importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            pick: .release(
                source: MappingFixtures.source,
                releaseId: "another-pressing"
            )
        )
        try await Task.sleep(for: .milliseconds(50))

        let candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.error != nil)
        #expect(candidate.pickInFlight == false)
        #expect(candidate.detail == before)
        #expect(candidate.hasSettled)
    }

    // 4c. Typing in a release field writes that one field, once, as the field
    //     is left. Nothing else about the form moves.
    @MainActor
    @Test("leaving a release field writes that field")
    func leavingAReleaseFieldWritesThatField() async throws {
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

        writer.setField(.year, "2011")
        try await Task.sleep(for: .milliseconds(50))

        #expect(recorder.editFields.count == 1)
        #expect(recorder.editFields.first?.field == .year)
        #expect(recorder.editFields.first?.value == "2011")
        // The store holds no copy of the form: it still reads what core last
        // answered with.
        #expect(
            store.selectedCandidates[MappingFixtures.candidateKey]?
                .edit?.pressing.year == MappingFixtures.albumEdit.pressing.year
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
            MappingFixtures.mapping(of: store).units[4].source.audioPath
        )

        actions.preview(entry)
        #expect(recorder.played == [MappingFixtures.containerPath])

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
                .rows[0]
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
        guard case .sheet(let sheet, let entries) = mapping.rows[0] else {
            Issue.record("expected a sheet row")
            return
        }
        #expect(sheet.assignment == .ignored)
        #expect(entries.isEmpty)
        #expect(mapping.willWriteCount == 1)
    }

    // 8. The identity toggle is the one control, and both directions are one
    //    pick core stores. Which side it shows is read off the stored pick, so
    //    the control cannot disagree with what will be committed.
    @MainActor
    @Test("switching release to Unknown and back is two picks")
    func switchingIdentityIsTwoPicks() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = Recorder()

        ImportSearchFlow.decideIdentity(
            importer: recorder.importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            pick: .unknown
        )
        try await Task.sleep(for: .milliseconds(50))
        #expect(recorder.picks == [.unknown])

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.unknownTable,
                picked: .unknown
            )
        )
        var candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.identity == .unknown)
        #expect(candidate.mapping.rows.count == 2)
        #expect(candidate.mapping.reconciliation == nil)
        #expect(candidate.pickedRelease == nil)

        ImportSearchFlow.decideIdentity(
            importer: recorder.importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            pick: MappingFixtures.pick
        )
        try await Task.sleep(for: .milliseconds(50))
        #expect(recorder.picks == [.unknown, MappingFixtures.pick])

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.thirteenFileTable
            )
        )
        candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.identity == .release)
        #expect(candidate.mapping.rows.count == 13)
        #expect(candidate.mapping.willWriteCount == 13)
        #expect(
            candidate.pickedRelease?.releaseId == MappingFixtures.releaseId
        )
    }
}
