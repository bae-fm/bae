import BaeKit
import Testing

@testable import bae

@MainActor
@Suite("Storage transfer inspector")
struct StorageTransferInspectorTests {
    @Test("each visible transfer queue carries its pause state")
    func visibleQueuesCarryPauseState() {
        let content = StorageTransferInspectorContent(
            releaseId: "rel-row-1",
            downloads: PreviewData.downloadSnapshot(paused: false),
            outputs: PreviewData.outputSnapshot(paused: true),
            outbox: PreviewData.outboxSnapshot(pauseState: .running)
        )

        #expect(content.items.map(\.pauseRequested) == [false, true, false])
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
        let content = StorageTransferInspectorContent(
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

        #expect(content.items.count == 3)
        for item in content.items {
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
        let content = StorageTransferInspectorContent(
            releaseId: "rel-selected",
            downloads: PreviewData.downloadSnapshot(ops: []),
            outputs: PreviewData.outputSnapshot(ops: []),
            outbox: PreviewData.outboxSnapshot(
                uploadGroups: [],
                deletes: PreviewData.deleteOps
            )
        )

        #expect(content.items.isEmpty)
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
