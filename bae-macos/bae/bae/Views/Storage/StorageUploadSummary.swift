import BaeKit
import SwiftUI

/// The selected release's upload: how many of its files are uploading,
/// queued and stored, with the button that cancels its upload; while files
/// are being sent, the bytes of its size stored or sent and how fast they go,
/// over a bar of the same bytes.
struct StorageUploadSummary: View {
    let releaseId: String
    let upload: BridgeReleaseUpload
    let onCancel: () -> Void

    /// The gap between the summary's lines, and its inset above and below.
    private static let lineGap: CGFloat = 10
    private static let verticalInset: CGFloat = 14

    var body: some View {
        VStack(alignment: .leading, spacing: Self.lineGap) {
            HStack(spacing: ThemeSpace.related) {
                Text(
                    QueueSummary.countLabel(
                        "core.queue.uploading",
                        upload.progress.uploading
                    )
                )
                .fontWeight(.semibold)
                .foregroundStyle(StorageTransferKind.upload.color)
                separator
                Text(
                    QueueSummary.countLabel(
                        "core.queue.queued",
                        upload.progress.queued
                    )
                )
                separator
                Text(
                    QueueSummary.countLabel("core.queue.stored", upload.stored)
                )
                .foregroundStyle(.secondary)
                Spacer()
                Button("Cancel", action: onCancel)
                    .controlSize(.small)
            }
            if let bar = upload.bar {
                HStack(alignment: .firstTextBaseline) {
                    Text(bar.text)
                    Spacer()
                    StorageReleaseUploadRate(releaseId: releaseId)
                }
                .foregroundStyle(.secondary)
                .monospacedDigit()
                ProgressTrackBar(
                    progress: bar.fraction,
                    tint: StorageTransferKind.upload.color
                )
            }
        }
        .themeText(.detail)
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, Self.verticalInset)
    }

    private var separator: some View {
        Text(verbatim: "\u{00B7}").foregroundStyle(.secondary)
    }
}

/// How fast the release's files being sent go together, read alone so a
/// confirmed part redraws only it. A zero reading shows nothing.
private struct StorageReleaseUploadRate: View {
    let releaseId: String
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        if let throughputBps = outboxStore.throughput(forRelease: releaseId),
            throughputBps > 0
        {
            Text(QueueSummary.throughputText(bytesPerSecond: throughputBps))
        }
    }
}
