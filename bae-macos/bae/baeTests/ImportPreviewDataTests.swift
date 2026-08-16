import BaeKit
import Testing

@testable import bae

@Suite("Import preview data")
struct ImportPreviewDataTests {
    @MainActor
    @Test("smoke preview combines every queue state and boundary tree")
    func smokePreviewCombinesProductionShapes() {
        let baseStore = PreviewData.importTabStore()
        let smokeStore = PreviewData.importSmokeTestStore()
        let entries = smokeStore.triageQueue.sections.flatMap(\.entries)
        let boundaryCount = entries.count {
            if case .boundary = $0 { return true }
            return false
        }

        #expect(smokeStore.watchedFolders.count == 2)
        #expect(boundaryCount == 5)
        #expect(smokeStore.queueIdentifyProgress?.identified == 20)
        #expect(
            smokeStore.queueIdentifyProgress?.total
                == smokeStore.triageQueue.counts.pending
                + smokeStore.triageQueue.counts.done
                + smokeStore.triageQueue.counts.skipped
        )
        #expect(
            smokeStore.triageQueue.counts.pending
                == baseStore.triageQueue.counts.pending + 4
        )
        #expect(
            smokeStore.triageQueue.counts.done
                == baseStore.triageQueue.counts.done
        )
        #expect(
            smokeStore.triageQueue.counts.skipped
                == baseStore.triageQueue.counts.skipped
        )
    }

    @MainActor
    @Test("smoke mapping represents every candidate file exactly")
    func smokeMappingMatchesCandidateFiles() throws {
        let candidate = PreviewData.importTabCandidate
        let mapping = try #require(candidate.mapping)
        let candidateFileIDs = Set(candidate.files.files.map(\.file.name))
        var representedFileIDs: Set<String> = []

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
            case .images(let images):
                representedFileIDs.formUnion(images.map(\.fileId))
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
        let entries = PreviewData.releaseBoundaryPreviewImportStore.triageQueue
            .sections
            .flatMap(\.entries)
        let boundaries = entries.compactMap {
            entry -> BridgeFolderReleaseBoundary? in
            guard case .boundary(_, let boundary) = entry else {
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
        let stores = [
            PreviewData.importTabStore(),
            PreviewData.importSmokeTestStore(),
            PreviewData.releaseQueueImportStore,
        ]
        for store in stores {
            let rows = store.triageQueue.sections.flatMap(\.entries)
                .compactMap {
                    entry -> BridgeTriageRow? in
                    guard case .candidate(_, let row) = entry else {
                        return nil
                    }
                    return row
                }

            #expect(!rows.isEmpty)
            #expect(
                rows.allSatisfy {
                    store.folderCandidates[$0.candidateKey] != nil
                }
            )
        }
    }

    @MainActor
    @Test("preview queue covers every pending question and terminal shape")
    func queueCoversProductionShapes() {
        let store = PreviewData.importTabStore()
        let entries = store.triageQueue.sections.flatMap(\.entries)
        let rows = entries.compactMap { entry -> BridgeTriageRow? in
            guard case .candidate(_, let row) = entry else { return nil }
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
