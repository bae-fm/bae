import BaeKit
import Testing

@testable import bae

@MainActor
@Suite("Storage transfer inspector")
struct StorageTransferInspectorTests {
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
            progress: PreviewData.uploadProgress(activity: .uploading)
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

        #expect(
            content.items.map(\.releaseId)
                == Array(
                    repeating: selectedReleaseId,
                    count: 3
                )
        )
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
        #expect(StorageTransferInspector.releaseId(in: []) == nil)
        #expect(
            StorageTransferInspector.releaseId(in: ["rel-selected"])
                == "rel-selected"
        )
        #expect(
            StorageTransferInspector.releaseId(
                in: ["rel-selected", "rel-other"]
            ) == nil
        )
    }

    @Test("closing clears inspector selection")
    func closingClearsInspectorSelection() {
        var selection: Set<String> = ["rel-selected"]

        StorageTransferInspector.close(selection: &selection)

        #expect(selection.isEmpty)
    }
}
