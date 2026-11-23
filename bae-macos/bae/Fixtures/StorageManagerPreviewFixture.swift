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
        answers: UnansweredCount = UnansweredCount(),
        selections: [Set<String>: BridgeStorageSelection] = [:],
        unloaded: Bool = false,
        queues: BridgeStorageQueues? = nil,
        inspectorOverrides: [String: BridgeStorageInspector] = [:]
    ) {
        let library = PreviewData.storageLibrary(
            answers: answers,
            rows: rows,
            downloads: downloads,
            outputs: outputs,
            uploads: uploads,
            selections: selections,
            unloaded: unloaded,
            queues: queues,
            inspectorOverrides: inspectorOverrides
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

    /// The environment the Storage Manager's views read, as the app's window
    /// installs it, over this fixture's stores unless `downloads`, `outputs`
    /// or `outbox` replace the transfer streams.
    func environment(
        of view: some View,
        downloads: DownloadStore? = nil,
        outputs: OutputStore? = nil,
        outbox: OutboxStore? = nil
    ) -> some View {
        view
            .environment(library)
            .environment(storageManagerStore)
            .environment(ImageStore.stub())
            .environment(ReleaseEditor.stub())
            .environment(Sync.stub())
            .environment(Downloads.stub())
            .environment(Outputs.stub())
            .environment(PreviewData.configStore())
            .environment(Importer.stub())
            .environment(uiStore)
            .environment(SettingsNavigation())
            .environment(\.openSettingsWindow, OpenSettingsWindowAction {})
            .environment(downloads ?? downloadStore)
            .environment(outputs ?? outputStore)
            .environment(outbox ?? outboxStore)
    }
}
