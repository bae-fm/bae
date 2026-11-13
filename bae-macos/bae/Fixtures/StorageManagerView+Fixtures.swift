import BaeKit
import Foundation
import SwiftUI

@testable import bae

@MainActor
struct StorageManagerPreviewFixture {
    let library: Library
    let storageManagerStore: StorageManagerStore
    let uiStore: UiStore
    let downloadStore: DownloadStore
    let outputStore: OutputStore
    let outboxStore: OutboxStore

    let initialSearch: String
    let initialSelection: Set<String>
    let initialInspectorPresented: Bool
    let initialSelectedFileId: String?

    init(
        rows: [BridgeStorageRow] = PreviewData.storageRows,
        search: String = "",
        selectedReleaseId: String? = nil,
        inspectorPresented: Bool = false,
        selectedFileId: String? = nil,
        downloads: [BridgeDownloadOp] = PreviewData.downloadOps,
        outputs: [BridgeOutputOp] = PreviewData.outputOps,
        uploads: [PreviewData.PreviewReleaseUpload] =
            PreviewData.defaultUploads,
        answers: UnansweredCount = UnansweredCount()
    ) {
        let library = PreviewData.storageLibrary(
            answers: answers,
            rows: rows,
            downloads: downloads,
            outputs: outputs,
            uploads: uploads
        )
        let uiStore = UiStore()
        self.library = library
        self.storageManagerStore = StorageManagerStore(
            library: library,
            onError: { uiStore.showError($0) }
        )
        self.uiStore = uiStore
        initialSearch = search
        initialSelection = selectedReleaseId.map { [$0] } ?? []
        initialInspectorPresented = inspectorPresented
        initialSelectedFileId = selectedFileId
        downloadStore = PreviewData.downloadStore(downloads: downloads)
        outputStore = PreviewData.outputStore()
        outboxStore = PreviewData.outboxStore()
    }
}

/// The window most shots draw in.
private enum PreviewWindow {
    static let width: CGFloat = 940
    static let height: CGFloat = 600
}
