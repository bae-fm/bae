import BaeKit
import Testing

@testable import bae

@MainActor
@Suite("Storage inspector")
struct StorageInspectorTests {
    @Test("uploads that name no release file are listed on their own rows")
    func uploadsWithoutAReleaseFileHaveTheirOwnRows() throws {
        let file = BridgeFile(
            id: "rel-row-1-audio",
            originalFilename: "disc/track.flac",
            fileSize: 26_000_000,
            contentType: "audio/flac",
            isImage: false,
            audioFormat: BridgeAudioFormat(
                codec: "FLAC",
                sampleRateHz: 44_100,
                bitsPerSample: 16,
                bitrateKbps: nil,
                channels: 2
            )
        )
        let rows = bridgeStorageInspectorFiles(
            releaseId: "rel-row-1",
            files: [file],
            outbox: PreviewData.outboxSnapshot()
        )
        let row = try #require(rows.first)
        #expect(rows.count == 1 + PreviewData.uploadFileOps.count)
        #expect(row.name == file.originalFilename)
        #expect(row.file?.audioFormat == file.audioFormat)
        #expect(row.operations.isEmpty)
        #expect(rows.contains { $0.operations.first?.label == .cover })

        let finished = bridgeStorageInspectorFiles(
            releaseId: "rel-row-1",
            files: [file],
            outbox: PreviewData.outboxSnapshot(uploadGroups: [])
        )
        #expect(finished.count == 1)
        #expect(finished.first?.identity == row.identity)
        #expect(finished.first?.operations.isEmpty == true)
        #expect(finished.first?.name == file.originalFilename)
    }

    @Test("another release's uploads do not enter the file list")
    func filesExcludeOtherReleases() {
        let rows = bridgeStorageInspectorFiles(
            releaseId: "rel-unselected",
            files: [],
            outbox: PreviewData.outboxSnapshot()
        )
        #expect(rows.isEmpty)
    }

    @Test("what an upload waits on reaches its row")
    func pendingUploadCarriesItsRecord() throws {
        let rows = bridgeStorageInspectorFiles(
            releaseId: "rel-row-3",
            files: [],
            outbox: PreviewData.outboxSnapshot(
                uploadGroups: [PreviewData.uploadGroupPending]
            )
        )
        let row = try #require(rows.first)
        let operation = try #require(row.operations.first)
        #expect(operation.pending?.reason.kind == .fileUnavailable)
    }

    @Test("each visible transfer queue carries its pause state")
    func visibleQueuesCarryPauseState() {
        let items = bridgeStorageInspectorTransfers(
            releaseId: "rel-row-1",
            downloads: PreviewData.downloadSnapshot(paused: false),
            outputs: PreviewData.outputSnapshot(paused: true),
            outbox: PreviewData.outboxSnapshot(pauseState: .running)
        )

        #expect(items.map(\.pauseRequested) == [false, true, false])
    }

    @Test("content contains only the selected release")
    func contentContainsOnlyTheSelectedRelease() {
        let selectedReleaseId = "rel-selected"
        let download = BridgeDownloadOp(
            releaseId: selectedReleaseId,
            release: BridgeQueuedRelease(
                title: "Album Title",
                fileCount: 2,
                totalSize: 2_048
            ),
            createdAt: 1,
            state: .queued
        )
        let output = BridgeOutputOp(
            releaseId: selectedReleaseId,
            targetDir: "/Music/Exports",
            release: BridgeQueuedRelease(
                title: "Album Title",
                fileCount: 2,
                totalSize: 2_048
            ),
            createdAt: 1,
            state: .queued,
            kind: .export
        )
        let upload = BridgeUploadReleaseGroup(
            releaseId: selectedReleaseId,
            files: [],
            progress: PreviewData.uploadProgress(activity: .uploading),
            throughputBps: 1_600_000
        )
        let items = bridgeStorageInspectorTransfers(
            releaseId: selectedReleaseId,
            downloads: PreviewData.downloadSnapshot(
                ops: PreviewData.downloadOps + [download]
            ),
            outputs: PreviewData.outputSnapshot(
                ops: PreviewData.outputOps + [output]
            ),
            outbox: PreviewData.outboxSnapshot(
                uploadGroups: [PreviewData.uploadGroup, upload]
            )
        )

        #expect(items.count == 3)
        for item in items {
            switch item {
            case .download(let operation, _):
                #expect(operation.releaseId == selectedReleaseId)
            case .output(let operation, _):
                #expect(operation.releaseId == selectedReleaseId)
            case .upload(let group, _):
                #expect(group.releaseId == selectedReleaseId)
            }
        }
    }

    @Test("inspector requires exactly one selected release")
    func inspectorRequiresExactlyOneSelectedRelease() {
        #expect(StorageInspector.releaseId(in: []) == nil)
        #expect(
            StorageInspector.releaseId(in: ["rel-selected"])
                == "rel-selected"
        )
        #expect(
            StorageInspector.releaseId(
                in: ["rel-selected", "rel-other"]
            ) == nil
        )
    }
}
