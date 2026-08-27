import BaeKit
import SwiftUI

/// Compact queue-wide download status and controls. Release-keyed operation
/// details live in `StorageTransferInspector`.
struct DownloadsSection: View {
    @Environment(DownloadStore.self)
    private var downloadStore
    @Environment(Downloads.self)
    private var downloads

    var body: some View {
        let snapshot = downloadStore.snapshot
        if !snapshot.downloads.isEmpty {
            Divider()
            QueueSectionHeader(
                icon: "arrow.down.circle",
                title: "Downloads",
                pauseRequested: snapshot.paused,
                pauseStatusText: snapshot.paused
                    ? String(localized: "Paused") : nil,
                summaryText: snapshot.summaryText,
                retryDisabled: snapshot.total.failed == 0,
                onSetPaused: { downloads.setDownloadsPaused($0) },
                onRetry: { downloads.retryDownloads() }
            )
        }
    }
}

#if DEBUG
    #Preview("Populated") {
        DownloadsSection()
            .environment(PreviewData.downloadStore())
            .environment(Downloads.stub())
            .frame(width: 680)
    }

    #Preview("Paused") {
        DownloadsSection()
            .environment(
                PreviewData.downloadStore(
                    PreviewData.downloadSnapshot(paused: true)
                )
            )
            .environment(Downloads.stub())
            .frame(width: 680)
    }
#endif
