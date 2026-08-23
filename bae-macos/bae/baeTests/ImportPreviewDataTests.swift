import BaeKit
import Testing

@testable import bae

@Suite("Import preview data")
struct ImportPreviewDataTests {
    @MainActor
    @Test("smoke preview combines every queue state and boundary tree")
    func smokePreviewCombinesProductionShapes() {
        let base = PreviewData.importTabScene()
        let smoke = PreviewData.importSmokeTestScene()
        let items = smoke.itemsByTab.values.flatMap { $0 }
        let boundaryCount = items.count {
            if case .boundary = $0 { return true }
            return false
        }

        #expect(smoke.store.watchedFolders.count == 2)
        #expect(boundaryCount == 5)
        #expect(smoke.store.queueIdentifyProgress?.identified == 20)
        #expect(
            smoke.store.queueIdentifyProgress?.total
                == smoke.store.summary.counts.pending
                + smoke.store.summary.counts.done
                + smoke.store.summary.counts.skipped
        )
        #expect(
            smoke.store.summary.counts.pending
                == base.store.summary.counts.pending + 4
        )
        #expect(
            smoke.store.summary.counts.done == base.store.summary.counts.done
        )
        #expect(
            smoke.store.summary.counts.skipped
                == base.store.summary.counts.skipped
        )
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
        let commitTracks = mapping.commitTracks

        #expect(candidate.files.files.count == 1)
        #expect(candidate.files.files[0].file.name == fileSources.first?.fileId)
        #expect(candidate.releaseDetailBridge?.trackCount == 10)
        #expect(candidate.editValues?.tracks.count == 10)
        #expect(fileSources.count == 1)
        #expect(missingSources == 9)
        #expect(mapping.reconciliation == .moreTracks(files: 1, tracks: 10))
        #expect(commitTracks.count == 10)
        #expect(commitTracks.count { $0.file != nil } == 1)
        #expect(commitTracks.count { $0.file == nil } == 9)
        #expect(candidate.commitEdit?.tracks == commitTracks)
    }

    @MainActor
    @Test("boundary preview covers several trees and every child kind")
    func boundaryPreviewCoversProductionShapes() {
        let items = PreviewData.releaseBoundaryScene()
            .itemsByTab[.pending] ?? []
        let boundaries = items.compactMap {
            item -> BridgeFolderReleaseBoundary? in
            guard case .boundary(_, let boundary) = item else {
                return nil
            }
            return boundary
        }
        let kinds = boundaries.flatMap(\.treeRows).map(\.kind)
        let invalidReasons = kinds.compactMap {
            kind -> BridgeInvalidReason? in
            guard case .invalid(let reason) = kind else { return nil }
            return reason
        }

        #expect(boundaries.count == 4)
        #expect(boundaries.contains { $0.sharedFileCount > 0 })
        #expect(boundaries.flatMap(\.treeRows).contains { $0.depth == 2 })
        #expect(
            kinds.contains {
                if case .folder = $0 { return true }
                return false
            }
        )
        #expect(
            kinds.contains {
                if case .candidate = $0 { return true }
                return false
            }
        )
        #expect(
            invalidReasons.contains(
                .corruptAudioFile(path: "Album 02.flac")
            )
        )
        #expect(invalidReasons.contains(.corruptImage(path: "Front.png")))
        #expect(invalidReasons.contains(.noValidAudio))
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
                    guard case .candidate(_, let row) = item else {
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
            guard case .candidate(_, let row) = item else { return nil }
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
        #expect(rows.contains { $0.placement == .done })
        #expect(rows.contains { $0.placement == .skipped })
        #expect(needsYouGroups.contains(.pickAPressing))
        #expect(needsYouGroups.contains(.signalsDisagree))
        #expect(needsYouGroups.contains(.countsOrLengthsDisagree))
        #expect(needsYouGroups.contains(.alreadyInLibrary))
        #expect(needsYouGroups.contains(.noMatch))
        #expect(needsYouGroups.contains(.stillIdentifying))
        #expect(
            entries.contains {
                if case .boundary = $0 { return true }
                return false
            }
        )
        #expect(
            entries.filter {
                if case .invalid = $0 { return true }
                return false
            }
            .count == 3
        )
    }
}
