import BaeKit
import SwiftUI

/// The selected release's pin while its download waits or runs: how many of
/// its files are kept and how many wait, with the button that cancels the
/// pin; while it downloads, the bytes it holds of its size and how fast it
/// goes, over a bar of the same bytes.
struct StoragePinSummary: View {
    let releaseId: String
    let files: BridgePinFiles
    let onCancel: () -> Void

    /// The gap between the summary's lines, and its inset above and below.
    private static let lineGap: CGFloat = 10
    private static let verticalInset: CGFloat = 14

    var body: some View {
        VStack(alignment: .leading, spacing: Self.lineGap) {
            HStack(spacing: ThemeSpace.related) {
                Text("Pinning")
                    .fontWeight(.semibold)
                    .foregroundStyle(StorageTransferKind.pin.color)
                separator
                Text(
                    QueueSummary.countLabel("core.queue.waiting", files.waiting)
                )
                separator
                Text(QueueSummary.countLabel("core.queue.pinned", files.kept))
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Cancel", action: onCancel)
                    .controlSize(.small)
            }
            StoragePinProgress(releaseId: releaseId)
        }
        .themeText(.detail)
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, Self.verticalInset)
    }

    private var separator: some View {
        Text(verbatim: "\u{00B7}").foregroundStyle(.secondary)
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
