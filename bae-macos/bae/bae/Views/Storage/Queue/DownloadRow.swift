import BaeKit
import SwiftUI

/// One download-queue row: album title, file count, size, a state badge, and a
/// cancel button.
struct DownloadRow: View {
    let op: BridgeDownloadOp
    /// The stream of how far the download has gone, which the row's native
    /// readout sets itself from while the download runs.
    let transfer: KeyedValueSource<BridgeDownloadTransferProgress>
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
            StatusChip(
                "Failed",
                tone: .danger,
                symbol: StatusTone.danger.symbol
            )
            .help(error)
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
                        .releaseTransfer(forRelease: op.releaseId),
                    onCancel: {}
                )
                Divider()
            }
        }
        .frame(width: 640)
        .padding(.vertical)
    }
#endif
