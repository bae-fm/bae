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
        case .download(let operation, let summary?):
            StoragePinSummary(
                summary: summary,
                download: StoragePinSummary.Download(
                    releaseId: operation.releaseId,
                    running: operation.isRunning
                ) {
                    run {
                        try await downloads.cancelDownload(operation.releaseId)
                    }
                }
            )
        case .pinFetch(let summary):
            StoragePinSummary(summary: summary, download: nil)
        case .download(let operation, nil):
            // A failed download: the row says why and offers its retry and
            // its cancel.
            DownloadRow(
                op: operation,
                transfer: downloadStore.downloadProgress(
                    forRelease: operation.releaseId
                ),
                onRetry: {
                    run { try await downloads.retryDownloads() }
                },
                onCancel: {
                    run {
                        try await downloads.cancelDownload(operation.releaseId)
                    }
                }
            )
        case .output(let operation):
            // The row's native label alone follows the percent the export
            // reaches.
            OutputRow(
                op: operation,
                percent: outputStore.exportProgress(
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

extension BridgeDownloadOp {
    /// Whether the download runs now, rather than waiting its turn or
    /// having failed.
    fileprivate var isRunning: Bool {
        if case .active = state { return true }
        return false
    }
}
