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
            MacTransferSummary(
                kind: .pin,
                counts: summary,
                releaseId: operation.releaseId,
                moving: operation.isRunning
            ) {
                run {
                    try await downloads.cancelDownload(operation.releaseId)
                }
            }
        case .download(let operation, nil):
            // A failed download: the row says why and offers its retry and
            // its cancel.
            DownloadRow(
                entry: .download(
                    operation,
                    transfer: operation.kind == .pin
                        ? .pin(
                            downloadStore.downloadProgress(
                                forRelease: operation.releaseId
                            )
                        )
                        : .moveToThisDevice(
                            downloadStore.moveToThisDeviceProgress(
                                forRelease: operation.releaseId
                            )
                        ),
                    onRetry: {
                        run {
                            try await downloads.retryDownload(
                                operation.releaseId
                            )
                        }
                    }
                ),
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
        case .upload(
            let releaseId,
            let title,
            let fileCount,
            let totalSize,
            let upload
        ):
            uploadTransfer(releaseId, title, fileCount, totalSize, upload)
        }
    }

    /// The release's upload: its counts with Cancel and its bytes while files
    /// move, or, while a missing original holds it, the row that says which
    /// file and offers restoring it.
    @ViewBuilder
    private func uploadTransfer(
        _ releaseId: String,
        _ title: String,
        _ fileCount: UInt64,
        _ totalSize: UInt64,
        _ upload: BridgeReleaseUpload
    ) -> some View {
        if let missing = upload.missing {
            DownloadRow(
                entry: .uploadWaiting(
                    title: title,
                    fileCount: fileCount,
                    totalSize: totalSize,
                    missing: missing
                )
            ) {
                run { try await sync.cancelReleaseTransition(releaseId) }
            }
        }
        else {
            MacTransferSummary(
                kind: .upload,
                counts: upload.summary,
                releaseId: releaseId,
                moving: upload.progress.uploading > 0
            ) {
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
