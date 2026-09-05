import BaeKit
import Testing

@testable import bae

@MainActor
@Suite("Storage inspector")
struct StorageInspectorTests {
    @Test("file metadata and upload progress occupy the same row")
    func filesCarryTheirOwnUpload() throws {
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
        #expect(rows.count == PreviewData.uploadFileOps.count)
        #expect(row.name == file.originalFilename)
        #expect(row.file?.audioFormat == file.audioFormat)
        #expect(row.upload?.state == .uploading)
        #expect(row.upload?.bar?.bytesDone == 12_400_000)
        #expect(
            row.throughputText
                == QueueSummary.throughputText(bytesPerSecond: 2_200_000)
        )
        #expect(row.sizeText == file.fileSizeText)
        #expect(rows.contains { $0.upload?.label == .cover })

        let finished = bridgeStorageInspectorFiles(
            releaseId: "rel-row-1",
            files: [file],
            outbox: PreviewData.outboxSnapshot(uploadGroups: [], deletes: [])
        )
        #expect(finished.count == 1)
        #expect(finished.first?.identity == row.identity)
        #expect(finished.first?.upload == nil)
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

    @Test("retry errors are visible on the file row")
    func fileRetryErrorIsVisible() throws {
        let rows = bridgeStorageInspectorFiles(
            releaseId: "rel-row-3",
            files: [],
            outbox: PreviewData.outboxSnapshot(
                uploadGroups: [PreviewData.uploadGroupSourceUnavailable],
                deletes: []
            )
        )
        let row = try #require(rows.first)
        #expect(row.progressText == "The source file is unavailable.")
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
            title: "Album Title",
            fileCount: 2,
            totalSize: 2_048,
            createdAt: 1,
            state: .queued
        )
        let output = BridgeOutputOp(
            releaseId: selectedReleaseId,
            targetDir: "/Music/Exports",
            title: "Album Title",
            fileCount: 2,
            totalSize: 2_048,
            createdAt: 1,
            state: .queued,
            kind: .export
        )
        let upload = BridgeUploadReleaseGroup(
            releaseId: selectedReleaseId,
            displayTitle: "Album Title",
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
                uploadGroups: [PreviewData.uploadGroup, upload],
                deletes: PreviewData.deleteOps
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

    @Test("cloud deletes are never attributed to a selected release")
    func cloudDeletesAreNeverAttributedToASelectedRelease() {
        let items = bridgeStorageInspectorTransfers(
            releaseId: "rel-selected",
            downloads: PreviewData.downloadSnapshot(ops: []),
            outputs: PreviewData.outputSnapshot(ops: []),
            outbox: PreviewData.outboxSnapshot(
                uploadGroups: [],
                deletes: PreviewData.deleteOps
            )
        )

        #expect(items.isEmpty)
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
