import BaeKit
import SwiftUI

@testable import bae

@MainActor
struct StorageManagerPreviewScene: View {
    private let fixture: StorageManagerPreviewFixture

    init(
        rows: [BridgeStorageRow] = PreviewData.storageRows,
        search: String = "",
        selectedReleaseId: String? = nil,
        inspectorPresented: Bool = false,
        selectedFileId: String? = nil,
        downloads: [BridgeDownloadOp] = PreviewData.downloadOps,
        outputs: [BridgeOutputOp] = PreviewData.outputOps,
        uploads: [PreviewData.PreviewReleaseUpload] =
            PreviewData.defaultUploads
    ) {
        fixture = StorageManagerPreviewFixture(
            rows: rows,
            search: search,
            selectedReleaseId: selectedReleaseId,
            inspectorPresented: inspectorPresented,
            selectedFileId: selectedFileId,
            downloads: downloads,
            outputs: outputs,
            uploads: uploads
        )
    }

    /// The scene's reads not yet answered.
    var answers: FixedAnswers { fixture.answers }

    /// The manager's store, which holds the list once its first page lands.
    var store: StorageManagerStore { fixture.storageManagerStore }

    var body: some View {
        StorageManagerView(
            initialSearch: fixture.initialSearch,
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented,
            initialSelectedFileId: fixture.initialSelectedFileId
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
