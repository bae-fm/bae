import BaeKit
import Testing

@testable import bae

/// Recording stubs for the two services the pane's actions reach.
@MainActor
private final class Recorder {
    var roleCalls: [(key: String, fileId: String, choice: BridgeFileRoleChoice)] =
        []
    var played: [String] = []
    var stops = 0

    var importer: Importer {
        Importer(
            setFileRole: { [self] key, fileId, choice in
                await MainActor.run {
                    roleCalls.append(
                        (key: key, fileId: fileId, choice: choice)
                    )
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
            onError: { _ in },
        )
    }
}

@Suite("Import mapping pane")
struct ImportMappingPaneTests {
    // 1. Binding a track sheet turns one container into the release's twelve
    //    slots. The pane re-reads the mapping the pick produces, so the folder
    //    goes from one slot to twelve without leaving the pane — and the twelve
    //    rows read as one run down the link column, because they are one file.
    @MainActor
    @Test("binding a sheet rebuilds the slot table")
    func bindingASheetRebuildsTheSlotTable() {
        let titles = (1...12).map { "Track \($0)" }
        let before = ImportMappingModel(
            files: MappingFixtures.emptyFiles,
            slots: MappingFixtures.oneSlotTable,
            edit: MappingFixtures.edit(
                titles: ["Track 1"],
                files: [.standalone(fileId: MappingFixtures.containerId)]
            )
        )
        #expect(before.slotRows.count == 1)
        #expect(before.willWriteCount == 1)
        #expect(before.reconciliation == .moreTracks(files: 1, tracks: 12))

        let after = ImportMappingModel(
            files: MappingFixtures.emptyFiles,
            slots: MappingFixtures.twelveSlotTable,
            edit: MappingFixtures.edit(
                titles: titles,
                files: (0..<12).map { MappingFixtures.slice($0).audio }
            )
        )
        #expect(after.slotRows.count == 12)
        #expect(after.willWriteCount == 12)
        #expect(after.reconciliation == .agrees(count: 12))
        // One file behind every row, drawn as one run.
        #expect(
            after.slotRows.allSatisfy {
                $0.file?.audio.fileId == MappingFixtures.containerId
            }
        )
        #expect(after.slotRows.first?.file?.span == .containerStart)
        #expect(after.slotRows.last?.file?.span == .containerEnd)
        #expect(
            after.slotRows.dropFirst().dropLast().allSatisfy {
                $0.file?.span == .containerMiddle
            }
        )
    }

    // 2. Thirteen files against a twelve-track source: the bar starts at
    //    thirteen tracks with one row unnamed, and typing a title leaves
    //    thirteen tracks with none unnamed. Naming a row never changes what
    //    will be written — only whether anyone has said what it is.
    @MainActor
    @Test("naming an unmatched file updates the commit bar")
    func namingAnUnmatchedFileUpdatesTheCommitBar() throws {
        let store = MappingFixtures.store(
            slots: MappingFixtures.thirteenFileSlots,
            edit: MappingFixtures.thirteenFileEdit
        )
        #expect(MappingFixtures.model(of: store).willWriteCount == 13)
        #expect(MappingFixtures.model(of: store).unansweredCount == 1)

        // Through the same binding the slot row's title field writes with.
        let candidate = try #require(
            store.folderCandidates[MappingFixtures.candidateKey]
        )
        let editor = ImportSearchFlow.makeEditValuesBinding(
            importStore: store,
            key: MappingFixtures.candidateKey,
            candidate: candidate
        )
        editor.wrappedValue.tracks[12].title = "Hidden Track"

        #expect(MappingFixtures.model(of: store).willWriteCount == 13)
        #expect(MappingFixtures.model(of: store).unansweredCount == 0)
    }

    // 3. Excluding a file takes its slot with it — thirteen rows back to
    //    twelve, and the tally back to agreement — and the decision is
    //    persisted rather than kept in the pane.
    @MainActor
    @Test("excluding a file removes its slot and restores the count")
    func excludingAFileRemovesItsSlot() async {
        let store = MappingFixtures.store(
            slots: MappingFixtures.thirteenFileSlots,
            edit: MappingFixtures.thirteenFileEdit
        )
        let recorder = Recorder()
        await ImportMappingFlow.exclude(
            key: MappingFixtures.candidateKey,
            fileId: "13.flac",
            services: recorder.services(store)
        )

        let model = MappingFixtures.model(of: store)
        #expect(model.slotRows.count == 12)
        #expect(model.willWriteCount == 12)
        #expect(model.unansweredCount == 0)
        #expect(model.reconciliation == .agrees(count: 12))
        #expect(model.audioChoices.count == 12)
        // Persisted, not a list edit: core is told, with the role that takes
        // the file out of the tracklist.
        #expect(recorder.roleCalls.count == 1)
        #expect(recorder.roleCalls.first?.fileId == "13.flac")
        #expect(recorder.roleCalls.first?.choice == .notATrack)
    }

    // 4. Nothing in the pane disables the commit. A row nobody named and a
    //    track with no audio behind it are both states core shapes into a
    //    committable edit — the button that used to grey out has no condition
    //    left to grey out on.
    @MainActor
    @Test("nothing disables the commit")
    func nothingDisablesTheCommit() {
        let unanswered = MappingFixtures.thirteenFileEdit
        #expect(MappingFixtures.isCommittable(unanswered))

        // A track the source names with nothing on disk behind it.
        var trackOnly = MappingFixtures.thirteenFileEdit
        trackOnly.tracks[5].file = nil
        #expect(MappingFixtures.isCommittable(trackOnly))

        let paneWithTrackOnly = ImportMappingModel(
            files: MappingFixtures.emptyFiles,
            slots: MappingFixtures.thirteenFileSlots,
            edit: trackOnly
        )
        // The row states its disagreement by writing nothing, and the rest of
        // the release still commits.
        #expect(paneWithTrackOnly.slotRows[5].file == nil)
        #expect(paneWithTrackOnly.willWriteCount == 12)
    }

    // 5. The slot row's play control auditions that row's own audio. For a
    //    sheet slice that is the container the slice is carved from, which is
    //    the only file on disk there is to play.
    @MainActor
    @Test("playing a slot's file works from the slot row")
    func playingASlotsFileWorksFromTheSlotRow() throws {
        let store = MappingFixtures.store(
            slots: MappingFixtures.twelveSlotTable,
            edit: MappingFixtures.edit(
                titles: (1...12).map { "Track \($0)" },
                files: (0..<12).map { MappingFixtures.slice($0).audio }
            )
        )
        let recorder = Recorder()
        let actions = ImportMappingFlow.slotActions(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        let row = MappingFixtures.model(of: store).slotRows[4]
        let path = try #require(row.file?.localPath)

        actions.preview(path)
        #expect(recorder.played == [MappingFixtures.containerPath])

        actions.stopPreview()
        #expect(recorder.stops == 1)
    }
}
