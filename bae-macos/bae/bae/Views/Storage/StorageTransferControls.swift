import BaeKit
import SwiftUI

/// The selected release's own transfers, as the inspector read them: its
/// download, its export, its upload. Pausing a whole queue is the footer's.
struct StorageTransferControls: View {
    @Environment(DownloadStore.self)
    private var downloadStore
    @Environment(OutputStore.self)
    private var outputStore
    @Environment(Downloads.self)
    private var downloads
    @Environment(Outputs.self)
    private var outputs
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    let items: [BridgeStorageInspectorTransfer]

    var body: some View {
        VStack(spacing: 0) {
            ForEach(items, id: \.queueId) { item in
                transfer(item)
                Divider()
            }
        }
    }

    @ViewBuilder
    private func transfer(_ item: BridgeStorageInspectorTransfer)
        -> some View
    {
        switch item {
        case .download(let operation, let files?):
            StoragePinSummary(
                files: files,
                download: StoragePinSummary.Download(
                    releaseId: operation.releaseId
                ) {
                    run {
                        try await downloads.cancelDownload(operation.releaseId)
                    }
                }
            )
        case .pinFetch(let files):
            StoragePinSummary(files: files, download: nil)
        case .download(let operation, nil):
            // A failed download: the row says why and offers its cancel.
            DownloadRow(
                op: operation,
                transfer: downloadStore.transfer(
                    forRelease: operation.releaseId
                )
            ) {
                run {
                    try await downloads.cancelDownload(operation.releaseId)
                }
            }
        case .output(let operation):
            // The row alone reads the percent the export reaches.
            OutputRow(
                op: operation,
                transfer: outputStore.transfer(
                    forRelease: operation.releaseId
                )
            ) {
                run { try await outputs.cancelOutput(operation.releaseId) }
            }
        case .upload(let releaseId, let upload):
            StorageUploadSummary(releaseId: releaseId, upload: upload) {
                run { try await sync.cancelReleaseTransition(releaseId) }
            }
        }
    }

    /// Run a transfer command, surfacing its failure in the Storage Manager
    /// window.
    private func run(_ command: @escaping @Sendable () async throws -> Void) {
        Task {
            do { try await command() }
            catch { uiStore.showError(error) }
        }
    }
}
