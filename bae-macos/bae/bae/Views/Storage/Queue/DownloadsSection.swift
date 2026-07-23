import BaeKit
import SwiftUI

/// Bottom-pane section showing the in-memory download (pin) queue: a header
/// band with the summary, a pause/resume toggle, and a "Retry now" action, plus
/// one row per release (title, file count, size, a Queued/Active/Failed
/// badge, and a cancel button). Hidden when the queue is idle. Reads
/// `DownloadStore` at the leaf; the download projection is the sole writer, so
/// actions don't optimistically mutate — a `.downloadQueue` invalidation
/// refetches and refreshes the section.
struct DownloadsSection: View {
    @Environment(DownloadStore.self)
    private var downloadStore
    @Environment(Downloads.self)
    private var downloads

    var body: some View {
        let snapshot = downloadStore.snapshot
        if !snapshot.downloads.isEmpty {
            Divider()
            VStack(spacing: 0) {
                QueueSectionHeader(
                    icon: "arrow.down.circle",
                    title: "Downloads",
                    paused: snapshot.paused,
                    summaryText: snapshot.summaryText,
                    retryDisabled: snapshot.total.failed == 0,
                    onSetPaused: { downloads.setDownloadsPaused($0) },
                    onRetry: { downloads.retryDownloads() }
                )
                Divider()
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(snapshot.downloads, id: \.releaseId) { op in
                            DownloadRow(op: op) {
                                downloads.cancelDownload(op.releaseId)
                            }
                            Divider()
                        }
                    }
                }
                .frame(maxHeight: 180)
            }
        }
    }

}

#if DEBUG
    #Preview("Populated") {
        DownloadsSection()
            .environment(PreviewData.downloadStore())
            .environment(Downloads.stub)
            .frame(width: 680)
    }

    #Preview("Paused") {
        DownloadsSection()
            .environment(
                PreviewData.downloadStore(
                    PreviewData.downloadSnapshot(paused: true)
                )
            )
            .environment(Downloads.stub)
            .frame(width: 680)
    }
#endif
