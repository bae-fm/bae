import BaeKit
import Testing

@testable import bae

@Suite("Import preview data")
struct ImportPreviewDataTests {
    @Test("only group members receive the child indent")
    func importListHierarchyInsets() {
        let root = ImportListHierarchyLayout.insets(isGroupMember: false)
        #expect(root.top == 0)
        #expect(root.leading == 0)
        #expect(root.bottom == 0)
        #expect(root.trailing == 0)

        let member = ImportListHierarchyLayout.insets(isGroupMember: true)
        #expect(member.top == 0)
        #expect(member.leading == 32)
        #expect(member.bottom == 0)
        #expect(member.trailing == 0)
    }

    @MainActor
    @Test("smoke preview marks every row below its group as a child")
    func smokePreviewExercisesGroupHierarchy() throws {
        let items = PreviewData.importTabItems(.pending)
        let groupIndex = try #require(
            items.firstIndex { item in
                if case .groupHeader = item { return true }
                return false
            }
        )

        let childFlags = items[(groupIndex + 1)...]
            .compactMap { item in
                if case .candidate(_, _, let isGroupMember) = item {
                    return isGroupMember
                }
                return nil
            }

        #expect(!childFlags.isEmpty)
        #expect(childFlags.allSatisfy { $0 })
    }

    @MainActor
    @Test("smoke preview includes a selectable ready child")
    func smokePreviewExercisesReadyGroupMember() {
        let hasReadyGroupMember = PreviewData.importTabItems(.pending)
            .contains { item in
                guard case .candidate(_, let row, true) = item else {
                    return false
                }
                return row.placement == .ready && row.selectable
            }

        #expect(hasReadyGroupMember)
    }

    @MainActor
    @Test("smoke preview combines every queue state")
    func smokePreviewCombinesProductionShapes() {
        let base = PreviewData.importTabScene()
        let smoke = PreviewData.importSmokeTestScene()
        // The smoke scene is the Import tab's own rows under a second watched
        // root: every state it draws is one the tab reads in production, and
        // the second root is the only thing it adds.
        #expect(smoke.store.watchedFolders.count == 2)
        #expect(
            smoke.store.summary.counts.pending
                == base.store.summary.counts.pending
        )
        #expect(
            smoke.store.summary.counts.done == base.store.summary.counts.done
        )
        #expect(
            smoke.store.summary.counts.skipped
                == base.store.summary.counts.skipped
        )
        // The identify count is over the whole queue, one row short of done.
        let queue =
            smoke.store.summary.counts.pending
            + smoke.store.summary.counts.done
            + smoke.store.summary.counts.skipped
        #expect(smoke.store.queueIdentifyProgress?.total == queue)
        #expect(smoke.store.queueIdentifyProgress?.identified == queue - 1)
    }

    @MainActor
    @Test("smoke mapping represents every candidate file exactly")
    func smokeMappingMatchesCandidateFiles() throws {
        let candidate = PreviewData.importTabCandidate
        let mapping = try #require(candidate.mapping)
        let candidateFileIDs = Set(candidate.files.files.map(\.file.name))
        var representedFileIDs = Set(mapping.images.map(\.fileId))

        for row in mapping.rows {
            switch row {
            case .unit(let unit):
                if case .file(let file) = unit.source {
                    representedFileIDs.insert(file.fileId)
                }
            case .sheet(let sheet, let entries):
                representedFileIDs.insert(sheet.sheetId)
                for entry in entries {
                    if case .sheetEntry(let source) = entry.source {
                        representedFileIDs.insert(source.containerId)
                    }
                }
            case .directory(let directory):
                representedFileIDs.formUnion(
                    candidate.files.files.compactMap { file in
                        file.file.dirPrefix == directory.dirPrefix
                            ? file.file.name : nil
                    }
                )
            }
        }

        #expect(representedFileIDs == candidateFileIDs)
    }

    @MainActor
    @Test("track mismatch preview keeps backed and missing rows distinct")
    func trackMismatchPreviewRepresentsTheSettledMapping() throws {
        let candidate = PreviewData.moreTracksMappingCandidate
        let mapping = try #require(candidate.mapping)
        let units = mapping.units
        let fileSources = units.compactMap { unit -> BridgeMappingFile? in
            guard case .file(let file) = unit.source else { return nil }
            return file
        }
        let missingSources = units.count {
            if case .missing = $0.source { return true }
            return false
        }
        let commitTracks = bridgeMappingTracks(table: mapping)

        #expect(candidate.files.files.count == 1)
        #expect(candidate.files.files[0].file.name == fileSources.first?.fileId)
        #expect(candidate.release?.trackCount == 10)
        #expect(fileSources.count == 1)
        #expect(missingSources == 9)
        #expect(mapping.reconciliation == .moreTracks(files: 1, tracks: 10))
        #expect(commitTracks.count == 10)
        #expect(commitTracks.count { $0.file != nil } == 1)
        #expect(commitTracks.count { $0.file == nil } == 9)
    }

    @MainActor
    @Test("every preview queue row opens its production candidate")
    func everyCandidateRowResolves() {
        let scenes = [
            PreviewData.importTabScene(),
            PreviewData.importSmokeTestScene(),
            PreviewData.releaseQueueScene(),
        ]
        for scene in scenes {
            let rows = scene.itemsByTab.values.flatMap { $0 }
                .compactMap {
                    item -> BridgeTriageRow? in
                    guard case .candidate(_, let row, _) = item else {
                        return nil
                    }
                    return row
                }

            #expect(!rows.isEmpty)
            #expect(
                rows.allSatisfy {
                    scene.store.selectedCandidates[$0.candidateKey] != nil
                }
            )
        }
    }

    @MainActor
    @Test("preview queue covers every pending question and terminal shape")
    func queueCoversProductionShapes() {
        let scene = PreviewData.importTabScene()
        let entries = scene.itemsByTab.values.flatMap { $0 }
        let rows = entries.compactMap { item -> BridgeTriageRow? in
            guard case .candidate(_, let row, _) = item else { return nil }
            return row
        }
        let needsYouGroups = rows.compactMap { row -> BridgeNeedsYouGroup? in
            guard case .needsYou(let group, _) = row.placement else {
                return nil
            }
            return group
        }

        #expect(rows.contains { $0.placement == .ready })
        #expect(rows.contains { $0.placement == .importing })
        #expect(rows.contains { $0.placement == .failed })
        #expect(rows.contains { $0.placement == .done })
        #expect(rows.contains { $0.placement == .skipped })
        #expect(needsYouGroups.contains(.pickAPressing))
        #expect(needsYouGroups.contains(.countsOrLengthsDisagree))
        #expect(needsYouGroups.contains(.alreadyInLibrary))
        #expect(needsYouGroups.contains(.noMatch))
        #expect(needsYouGroups.contains(.stillIdentifying))
        #expect(
            entries.filter {
                if case .invalid = $0 { return true }
                return false
            }
            .count == 3
        )
    }
}
