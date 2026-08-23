import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Recording stubs for the services the mapping table's controls reach.
@MainActor
private final class Recorder {
    var roleCalls:
        [(key: String, fileId: String, choice: BridgeFileRoleChoice)] =
            []
    var bindCalls: [(sheetFileId: String, audioFileId: String?)] = []
    var discCalls: [(sheetFileId: String, disc: BridgeSheetDisc)] = []
    var played: [String] = []
    var stops = 0
    /// What a re-read of the folder's mapping comes back with — the table core
    /// projects once the decision under test has landed.
    var remapped: BridgeMappingTable = MappingFixtures.thirteenFileTable
    /// What reading the folder as Unknown comes back with.
    var unknown: BridgeUnknownMapping = BridgeUnknownMapping(
        seed: MappingFixtures.albumSeed,
        mapping: MappingFixtures.unknownTable
    )

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
                await MainActor.run { decided(for: pick) }
            },
            candidateDecidedIdentity: { [self] _ in
                await MainActor.run {
                    stored.map { decided(for: $0) }
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
            }
        )
    }

    /// The pick a stored decision would resume — `nil` reads as "nothing
    /// decided". Defaults to the fixture release, which is what a candidate
    /// with `MappingFixtures.pick` in force has stored.
    var stored: BridgeIdentityPick? = .release(
        source: MappingFixtures.pick.source,
        releaseId: MappingFixtures.pick.releaseId,
        claim: MappingFixtures.pick.claim
    )

    /// The answer either identity stands for, from the same fixtures the old
    /// per-call stubs served. A release pick comes back claimed at the level it
    /// carried, which is what core does with it.
    private func decided(for pick: BridgeIdentityPick) -> BridgeDecidedIdentity
    {
        switch pick {
        case .release(let source, let releaseId, let claim):
            .release(
                source: source,
                releaseId: releaseId,
                prefetch: MappingFixtures.prefetch(
                    mapping: remapped,
                    level: claim
                )
            )
        case .unknown:
            .unknown(seed: unknown.seed, mapping: unknown.mapping)
        }
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
                .environment(PreviewData.importTabImporter()),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let restoredConflict = try await SnapshotTestSupport.capturePNG(
            host,
            size: size
        )

        store.mutateCandidate(forKey: key) {
            $0.identifyState = .manualOnly(trackCount: 11)
        }
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let before = try await SnapshotTestSupport.capturePNG(host, size: size)

        #expect(restoredConflict != before)

        store.mutateCandidate(forKey: key) {
            $0.identifyState = PreviewData.searchStateConflict.identifyState
        }
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let conflict = try await SnapshotTestSupport.capturePNG(
            host,
            size: size
        )

        #expect(conflict != before)

        store.mutateCandidate(forKey: key) {
            $0.identityChoice = .exact(
                releaseId: "restored-release",
                source: .musicBrainz
            )
        }
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let settledConflict = try await SnapshotTestSupport.capturePNG(
            host,
            size: size
        )

        store.mutateCandidate(forKey: key) {
            $0.identifyState = .manualOnly(trackCount: 11)
        }
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let settledWithoutConflict = try await SnapshotTestSupport.capturePNG(
            host,
            size: size
        )

        #expect(settledConflict == settledWithoutConflict)
    }

    // 1. Binding a track sheet turns one container into the release's twelve
    //    entries. The pane re-reads the mapping the pick produces, so the
    //    folder goes from one row carrying audio to twelve without leaving the
    //    pane — and the twelve are the sheet's own group.
    @MainActor
    @Test("binding a sheet rebuilds the mapping table")
    func bindingASheetRebuildsTheTable() async {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.unboundSheetTable
        )
        #expect(MappingFixtures.mapping(of: store).willWriteCount == 1)
        #expect(
            MappingFixtures.mapping(of: store).reconciliation
                == .moreTracks(files: 1, tracks: 12)
        )

        let recorder = Recorder()
        recorder.remapped = MappingFixtures.boundSheetTable()
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

    // 2. Thirteen files against a twelve-track release: the bar starts at
    //    thirteen tracks with one row unnamed, and typing a title leaves
    //    thirteen tracks with none unnamed. Naming a row never changes what
    //    will be written — only whether anyone has said what it is.
    @MainActor
    @Test("naming an unmatched row updates the commit bar")
    func namingAnUnmatchedRowUpdatesTheCommitBar() throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        #expect(MappingFixtures.mapping(of: store).willWriteCount == 13)
        #expect(MappingFixtures.mapping(of: store).unansweredCount == 1)

        // Through the same call the row's title field writes with.
        let unnamed = try #require(
            MappingFixtures.mapping(of: store).units.last?.track
        )
        var named = unnamed
        named.title = "Hidden Track"
        ImportMappingFlow.editTrack(
            key: MappingFixtures.candidateKey,
            track: named,
            importStore: store
        )

        #expect(MappingFixtures.mapping(of: store).willWriteCount == 13)
        #expect(MappingFixtures.mapping(of: store).unansweredCount == 0)
    }

    // 3. Excluding a file takes its row with it — thirteen rows back to twelve,
    //    and the tally back to agreement — and the decision is persisted rather
    //    than kept in the pane.
    @MainActor
    @Test("excluding a file removes its row and restores the count")
    func excludingAFileRemovesItsRow() async {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = Recorder()
        await ImportMappingFlow.exclude(
            key: MappingFixtures.candidateKey,
            fileId: "13.flac",
            services: recorder.services(store)
        )

        let mapping = MappingFixtures.mapping(of: store)
        #expect(mapping.rows.count == 12)
        #expect(mapping.willWriteCount == 12)
        #expect(mapping.unansweredCount == 0)
        #expect(mapping.reconciliation == .agrees(count: 12))
        #expect(mapping.audioChoices.count == 12)
        // Persisted, not a list edit: core is told, with the role that takes
        // the file out of the tracklist.
        #expect(recorder.roleCalls.count == 1)
        #expect(recorder.roleCalls.first?.fileId == "13.flac")
        #expect(recorder.roleCalls.first?.choice == .notATrack)
    }

    // 3b. Excluding the audio a sheet describes takes the whole group with it:
    //     twelve entries are one file's rows, and the file is leaving.
    @MainActor
    @Test("excluding a sheet's container removes the whole group")
    func excludingASheetsContainerRemovesTheGroup() async {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.boundSheetTable()
        )
        let recorder = Recorder()
        await ImportMappingFlow.exclude(
            key: MappingFixtures.candidateKey,
            fileId: MappingFixtures.containerId,
            services: recorder.services(store)
        )

        let mapping = MappingFixtures.mapping(of: store)
        #expect(mapping.rows.isEmpty)
        #expect(mapping.willWriteCount == 0)
        #expect(recorder.roleCalls.first?.choice == .notATrack)
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
            MappingFixtures.mapping(of: unanswered).commitTracks.count == 13
        )

        // A release that names more tracks than the folder has anything for.
        let unbacked = MappingFixtures.store(
            mapping: MappingFixtures.unboundSheetTable
        )
        #expect(MappingFixtures.isCommittable(unbacked))
        #expect(MappingFixtures.mapping(of: unbacked).willWriteCount == 1)
        #expect(MappingFixtures.mapping(of: unbacked).commitTracks.count == 12)
    }

    // 4b. A re-pick that fails unsettles the identity: the table and the album
    //     fields stay put, but there is nothing to commit them under, so the
    //     commit bar has nothing to render and the commit has nothing to read.
    @MainActor
    @Test("a failed re-pick leaves nothing to commit")
    func aFailedRepickLeavesNothingToCommit() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        #expect(MappingFixtures.isCommittable(store))

        ImportSearchFlow.decideIdentity(
            importer: Importer(pickCandidateIdentity: { _, _ in
                throw StubError.notImplemented
            }),
            importStore: store,
            key: MappingFixtures.candidateKey,
            pick: .release(
                source: MappingFixtures.pick.source,
                releaseId: MappingFixtures.pick.releaseId,
                claim: .exact
            )
        )
        try await Task.sleep(for: .milliseconds(50))

        let candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.identityChoice == nil)
        #expect(candidate.mapping != nil)
        #expect(candidate.editValues != nil)
        #expect(candidate.commitEdit == nil)
    }

    // 4c. Claiming exactly this pressing is a claim about the values on the
    //     screen, so editing one of them away lowers the claim by itself — and
    //     the commit carries the lowered one, because it is the same state.
    @MainActor
    @Test("editing the pressing lowers the claim the commit carries")
    func editingThePressingLowersTheClaim() throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.claim?.level == .exact)

        // Through the same binding the release fields write with.
        let editor = ImportSearchFlow.makeEditValuesBinding(
            importStore: store,
            key: MappingFixtures.candidateKey,
            candidate: candidate
        )
        var edited = editor.wrappedValue
        edited.pressing.year = "2011"
        editor.wrappedValue = edited

        let after = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(after.claim?.level == .approximate)
        #expect(
            after.identityChoice
                == .approximate(
                    releaseId: MappingFixtures.pick.releaseId,
                    source: MappingFixtures.pick.source
                )
        )
        // Editing anything else says nothing about which pressing is held.
        var titled = after.editValues ?? edited
        titled.albumTitle = "Album Title Two"
        editor.wrappedValue = titled
        #expect(
            store.selectedCandidates[MappingFixtures.candidateKey]?.claim?.level
                == .approximate
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
    //    say. Saying it persists the decision and comes back as the tracklist
    //    it re-shapes.
    @MainActor
    @Test("assigning a cue to a disc persists it and re-reads the table")
    func assigningACueToADisc() async {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.boundSheetTable()
        )
        let recorder = Recorder()
        recorder.remapped = MappingFixtures.boundSheetTable(
            assignment: .disc(number: 2)
        )
        await ImportMappingFlow.setSheetDisc(
            key: MappingFixtures.candidateKey,
            sheetFileId: MappingFixtures.sheetId,
            disc: .disc(number: 2),
            services: recorder.services(store)
        )

        #expect(recorder.discCalls.count == 1)
        #expect(recorder.discCalls.first?.disc == .disc(number: 2))
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
        recorder.remapped = MappingFixtures.ignoredSheetTable
        await ImportMappingFlow.setSheetDisc(
            key: MappingFixtures.candidateKey,
            sheetFileId: MappingFixtures.sheetId,
            disc: .ignored,
            services: recorder.services(store)
        )

        #expect(recorder.discCalls.first?.disc == .ignored)
        let mapping = MappingFixtures.mapping(of: store)
        guard case .sheet(let sheet, let entries) = mapping.rows[0] else {
            Issue.record("expected a sheet row")
            return
        }
        #expect(sheet.assignment == .ignored)
        #expect(entries.isEmpty)
        #expect(mapping.willWriteCount == 1)
    }

    // 8. The identity toggle is the one control, and both directions leave a
    //    table to work in: Unknown reads the folder's own tags, and switching
    //    back re-picks the release the candidate already held.
    @MainActor
    @Test("switching release to Unknown and back keeps the table populated")
    func switchingIdentityKeepsTheTablePopulated() async throws {
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

        var candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.identity == .unknown)
        #expect(candidate.identityChoice == .unknown)
        #expect(candidate.mapping?.rows.count == 2)
        #expect(candidate.mapping?.reconciliation == nil)
        // The release it came from is still held, which is what switching back
        // re-picks rather than sending the user back to the search.
        #expect(candidate.pick == MappingFixtures.pick)

        let pick = try #require(candidate.pick)
        recorder.remapped = MappingFixtures.thirteenFileTable
        ImportSearchFlow.decideIdentity(
            importer: recorder.importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            pick: .release(
                source: pick.source,
                releaseId: pick.releaseId,
                claim: pick.claim
            )
        )
        try await Task.sleep(for: .milliseconds(50))

        candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.identity == .release)
        #expect(candidate.mapping?.rows.count == 13)
        #expect(candidate.mapping?.willWriteCount == 13)
    }

    // 9. Lowering the claim is a re-pick of the same release at the album
    //    level: the claim is part of the decision core stores, so it lands the
    //    same way a pick does and the commit carries it.
    @MainActor
    @Test("lowering the claim re-picks the release at the album level")
    func loweringTheClaimRePicksTheRelease() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = Recorder()

        ImportSearchFlow.decideIdentity(
            importer: recorder.importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            pick: .release(
                source: MappingFixtures.pick.source,
                releaseId: MappingFixtures.pick.releaseId,
                claim: .approximate
            )
        )
        try await Task.sleep(for: .milliseconds(50))

        let candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.claim?.level == .approximate)
        #expect(
            candidate.identityChoice
                == .approximate(
                    releaseId: MappingFixtures.pick.releaseId,
                    source: MappingFixtures.pick.source
                )
        )
        // The same release, still picked — lowering the claim says the
        // pressing is not vouched for, not that the release is wrong.
        #expect(candidate.pick?.releaseId == MappingFixtures.pick.releaseId)
        #expect(candidate.pick?.claim == .approximate)
    }

    // 9b. And it survives the round trip through the folder's own tags:
    //     switching back re-picks at the level the user set, because the
    //     candidate's held pick carries it rather than defaulting again.
    @MainActor
    @Test("a lowered claim survives switching to Unknown and back")
    func aLoweredClaimSurvivesTheUnknownRoundTrip() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = Recorder()

        ImportSearchFlow.decideIdentity(
            importer: recorder.importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            pick: .release(
                source: MappingFixtures.pick.source,
                releaseId: MappingFixtures.pick.releaseId,
                claim: .approximate
            )
        )
        try await Task.sleep(for: .milliseconds(50))

        ImportSearchFlow.decideIdentity(
            importer: recorder.importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            pick: .unknown
        )
        try await Task.sleep(for: .milliseconds(50))

        var candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        let held = try #require(candidate.pick)
        #expect(held.claim == .approximate)

        // Exactly what the identity control sends to switch back.
        ImportSearchFlow.decideIdentity(
            importer: recorder.importer,
            importStore: store,
            key: MappingFixtures.candidateKey,
            pick: .release(
                source: held.source,
                releaseId: held.releaseId,
                claim: held.claim
            )
        )
        try await Task.sleep(for: .milliseconds(50))

        candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        #expect(candidate.claim?.level == .approximate)
    }
}
