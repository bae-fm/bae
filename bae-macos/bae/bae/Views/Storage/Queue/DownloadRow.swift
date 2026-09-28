import BaeKit
import SwiftUI

/// One download-queue row: album title, file count, size, a state badge, and a
/// cancel button.
struct DownloadRow: View {
    let op: BridgeDownloadOp
    let onCancel: () -> Void

    var body: some View {
        QueueRow(
            icon: "arrow.down.circle",
            createdAt: op.createdAt,
            cancel: .init(help: "Cancel this download", action: onCancel)
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                HStack(spacing: ThemeSpace.related) {
                    Text(op.title)
                        .lineLimit(1)

                    Text(op.detailText)
                        .themeText(.detail)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
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
        if case .active(let progress) = op.state {
            DownloadTransferProgressView(progress: progress)
        }
    }
}

#if DEBUG
    #Preview("Download states") {
        VStack(spacing: 0) {
            ForEach(PreviewData.downloadOps, id: \.releaseId) { op in
                DownloadRow(op: op, onCancel: {})
                Divider()
            }
        }
        .frame(width: 640)
        .padding(.vertical)
    }
#endif
