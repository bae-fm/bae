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
            cancelHelp: "Cancel this download",
            onCancel: onCancel
        ) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(op.title)
                        .lineLimit(1)

                    Text(op.detailText)
                        .font(.caption)
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
            Label("Queued", systemImage: "clock")
                .foregroundStyle(.secondary)
        case .active:
            Label(
                "Downloading",
                systemImage: "arrow.down.circle.fill"
            )
            .foregroundStyle(.orange)
        case .failed(let error):
            Label("Failed", systemImage: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
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
