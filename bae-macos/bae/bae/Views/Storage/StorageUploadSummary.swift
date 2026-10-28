import BaeKit
import SwiftUI

/// The selected release's upload: how many of its files are uploading,
/// queued and stored, as core counts them, with the button that cancels its
/// upload; while files are being sent, the bytes of its size stored or sent
/// and how fast they go, over a bar of the same bytes, read from its
/// transfer.
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
                StorageTransferCounts(counts: upload.summary, kind: .upload)
                Spacer()
                Button("Cancel", action: onCancel)
                    .controlSize(.small)
            }
            StorageReleaseUploadProgress(releaseId: releaseId)
        }
        .themeText(.detail)
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, Self.verticalInset)
    }
}

/// The bytes the release holds of its size and how fast its files being sent
/// go, over a bar of the same bytes, read alone by its release so moving
/// bytes redraw only it; nothing while none of its files is being sent. A
/// zero rate shows nothing.
private struct StorageReleaseUploadProgress: View {
    let releaseId: String
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        if let transfer = outboxStore.transfer(forRelease: releaseId) {
            HStack(alignment: .firstTextBaseline) {
                Text(transfer.bar.text)
                Spacer()
                if transfer.throughputBps > 0 {
                    Text(
                        QueueSummary.throughputText(
                            bytesPerSecond: transfer.throughputBps
                        )
                    )
                }
            }
            .foregroundStyle(.secondary)
            .monospacedDigit()
            ProgressTrackBar(
                progress: transfer.bar.fraction,
                tint: StorageTransferKind.upload.color
            )
        }
    }
}
