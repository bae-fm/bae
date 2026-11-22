import BaeKit
import SwiftUI

/// The selected release's own transfers, as the inspector read them: its
/// download, its export, its upload. Pausing a whole queue is the footer's.
struct StorageTransferControls: View {
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
    @Environment(\.currentMoment)
    private var currentMoment

    let items: [BridgeStorageInspectorTransfer]

    var body: some View {
        // Read once, so every row words its queued time against the same
        // moment.
        let now = currentMoment
        VStack(spacing: 0) {
            ForEach(items, id: \.queueId) { item in
                transfer(item, now: now)
                Divider()
            }
        }
    }

    @ViewBuilder
    private func transfer(
        _ item: BridgeStorageInspectorTransfer,
        now: Date
    ) -> some View {
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
            failedDownload(operation)
        case .output(let operation):
            // The row's native label alone follows the percent the export
            // reaches.
            OutputRow(
                op: operation,
                now: now,
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
        case .moveFailed(
            let releaseId,
            let title,
            let fileCount,
            let totalSize,
            let changed,
            let detail
        ):
            moveFailedTransfer(
                releaseId,
                MacFailedTransferRow.Entry.moveFailed(
                    title: title,
                    fileCount: fileCount,
                    totalSize: totalSize,
                    changed: changed,
                    reason: DisplayError(
                        line: String(localized: "Could not move to the cloud"),
                        detail: detail
                    )
                )
            )
        }
    }

    /// A failed download: the row says why and offers its retry and its
    /// cancel.
    private func failedDownload(_ operation: BridgeDownloadOp) -> some View {
        MacFailedTransferRow(
            entry: .download(
                operation,
                onRetry: {
                    run {
                        try await downloads.retryDownload(operation.releaseId)
                    }
                }
            ),
            onCancel: {
                run { try await downloads.cancelDownload(operation.releaseId) }
            }
        )
    }

    /// The release's move to the cloud that failed: the file whose original
    /// changed, with Details and the cancel that ends the move.
    private func moveFailedTransfer(
        _ releaseId: String,
        _ entry: MacFailedTransferRow.Entry
    ) -> some View {
        MacFailedTransferRow(entry: entry) {
            run { try await sync.cancelReleaseTransition(releaseId) }
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
            MacFailedTransferRow(
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
