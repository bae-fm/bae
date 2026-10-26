import BaeKit
import SwiftUI

@testable import bae

@MainActor
struct StorageManagerPreviewScene: View {
    private let fixture: StorageManagerPreviewFixture

    init(
        rows: [BridgeStorageRow] = PreviewData.storageRows,
        selectedReleaseId: String? = nil,
        inspectorPresented: Bool = false,
        downloads: [BridgeDownloadOp] = PreviewData.downloadOps,
        outputs: [BridgeOutputOp] = PreviewData.outputOps,
        uploadGroups: [BridgeUploadReleaseGroup] =
            PreviewData.defaultUploadGroups
    ) {
        fixture = StorageManagerPreviewFixture(
            rows: rows,
            selectedReleaseId: selectedReleaseId,
            inspectorPresented: inspectorPresented,
            downloads: downloads,
            outputs: outputs,
            uploadGroups: uploadGroups
        )
    }

    var body: some View {
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
    }
}
