import BaeKit
import SwiftUI

/// The selected release's pin while its files come to this device: how many
/// of its files are downloading, waiting and kept, as core counts them. While its
/// download waits or runs, the button that cancels the pin, and while it
/// downloads, the bytes it holds of its size and how fast it goes, over a bar
/// of the same bytes. While coven fetches the copies its pins wait for,
/// outside the download queue, the counts alone: no queue entry holds that
/// fetch to cancel or measure.
struct StoragePinSummary: View {
    let summary: [BridgeTransferCount]
    let download: Download?

    /// The download pinning the release, and what cancels it.
    struct Download {
        let releaseId: String
        let onCancel: () -> Void
    }

    /// The gap between the summary's lines, and its inset above and below.
    private static let lineGap: CGFloat = 10
    private static let verticalInset: CGFloat = 14

    var body: some View {
        VStack(alignment: .leading, spacing: Self.lineGap) {
            HStack(spacing: ThemeSpace.related) {
                StorageTransferCounts(counts: summary, kind: .pin)
                Spacer()
                if let download {
                    Button("Cancel", action: download.onCancel)
                        .controlSize(.small)
                }
            }
            if let download {
                StoragePinProgress(releaseId: download.releaseId)
            }
        }
        .themeText(.detail)
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, Self.verticalInset)
    }
}

/// The bytes the release's download holds of its size and how fast it has
/// gone, over a bar of the same bytes, read alone by its release; nothing
/// while the release waits its turn.
private struct StoragePinProgress: View {
    let releaseId: String
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        if let progress = downloadStore.transfer(forRelease: releaseId)?
            .progress
        {
            HStack(alignment: .firstTextBaseline) {
                Text(progress.bytesText)
                Spacer()
                if let rate = progress.throughputBps {
                    Text(QueueSummary.throughputText(bytesPerSecond: rate))
                }
            }
            .foregroundStyle(.secondary)
            .monospacedDigit()
            ProgressTrackBar(
                progress: progress.fraction,
                tint: StorageTransferKind.pin.color
            )
        }
    }
}
