import BaeKit
import SwiftUI

/// One download-queue row: album title, file count, size, a state badge, and a
/// cancel button; once the download failed, a button that queues every failed
/// download again.
struct DownloadRow: View {
    let op: BridgeDownloadOp
    /// The latest progress of the download, which the row's native readout
    /// reads each display frame while the download runs.
    let transfer: LatestSource<BridgeDownloadTransferProgress>
    /// Core retries the queue's failures together, not this entry alone.
    let onRetry: () -> Void
    let onCancel: () -> Void

    var body: some View {
        QueueRow(
            icon: "arrow.down.circle",
            createdAt: op.createdAt,
            cancel: .init(help: "Cancel this download", action: onCancel)
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                HStack(spacing: ThemeSpace.related) {
                    Text(op.titleText)
                        .lineLimit(1)

                    if let release = op.release {
                        Text(release.detailText)
                            .themeText(.detail)
                            .monospacedDigit()
                            .foregroundStyle(.secondary)
                    }
                }
                progressView
            }
        } badge: {
            stateBadge
        }
    }

    @ViewBuilder
    private var stateBadge: some View {
        switch op.state {
        case .queued:
            StatusChip("Queued", symbol: "clock")
        case .active:
            StatusChip(
                "Downloading",
                tone: .activity,
                symbol: "arrow.down.circle.fill"
            )
        case .failed(let error):
            HStack(spacing: ThemeSpace.related) {
                StatusChip(
                    "Failed",
                    tone: .danger,
                    symbol: StatusTone.danger.symbol
                )
                .help(error)
                Button("Retry", action: onRetry)
                    .controlSize(.small)
            }
        }
    }

    @ViewBuilder
    private var progressView: some View {
        if case .active = op.state {
            TransferReadout(stream: .pin(transfer), kind: .pin)
        }
    }
}

#if DEBUG
    #Preview("Download states") {
        VStack(spacing: 0) {
            ForEach(PreviewData.downloadOps, id: \.releaseId) { op in
                DownloadRow(
                    op: op,
                    transfer: PreviewData.downloadStore()
                        .downloadProgress(forRelease: op.releaseId),
                    onRetry: {},
                    onCancel: {}
                )
                Divider()
            }
        }
        .frame(width: 640)
        .padding(.vertical)
    }
#endif
