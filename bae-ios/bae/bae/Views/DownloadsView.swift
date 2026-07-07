import BaeKit
import SwiftUI

/// Download-queue management: every queued/active/failed pin with progress,
/// pause/resume for the whole queue, retry for failed entries, per-item
/// cancel via swipe. Reads `DownloadStore` at the leaf; actions never mutate
/// optimistically — the next queue snapshot re-renders the list.
struct DownloadsView: View {
    @Environment(DownloadStore.self)
    private var downloadStore
    @Environment(Downloads.self)
    private var downloads
    @Environment(\.dismiss)
    private var dismiss

    var body: some View {
        let snapshot = downloadStore.snapshot
        NavigationStack {
            Group {
                if snapshot.downloads.isEmpty {
                    // The queue is transient; it can drain while the sheet is
                    // open. Show an empty state rather than dismissing out from
                    // under the user.
                    ContentUnavailableView(
                        "No downloads",
                        systemImage: "arrow.down.circle"
                    )
                }
                else {
                    List {
                        Section {
                            ForEach(snapshot.downloads, id: \.releaseId) { op in
                                DownloadQueueRow(op: op)
                                    .swipeActions(edge: .trailing) {
                                        Button("Cancel", role: .destructive) {
                                            downloads.cancelDownload(
                                                op.releaseId
                                            )
                                        }
                                    }
                            }
                        } header: {
                            header(snapshot)
                        }
                        .textCase(nil)
                    }
                    .listStyle(.plain)
                }
            }
            .navigationTitle("Downloads")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button(
                        snapshot.paused
                            ? String(localized: "Resume")
                            : String(localized: "Pause")
                    ) {
                        downloads.setDownloadsPaused(!snapshot.paused)
                    }
                    .disabled(snapshot.downloads.isEmpty)
                }
                ToolbarItem(placement: .topBarLeading) {
                    Button("Retry") {
                        downloads.retryDownloads()
                    }
                    .disabled(snapshot.total.failed == 0)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    @ViewBuilder
    private func header(_ snapshot: BridgeDownloadSnapshot) -> some View {
        DownloadQueueSummaryLine(snapshot: snapshot, compact: false)
    }
}

/// One line summarizing the download queue: a "Paused" chip when the user has
/// paused it, otherwise the count summary (downloading / failed / queued).
/// `compact` shrinks it to caption size for the library strip.
struct DownloadQueueSummaryLine: View {
    let snapshot: BridgeDownloadSnapshot
    let compact: Bool

    var body: some View {
        Group {
            if snapshot.paused {
                Label("Paused", systemImage: "pause.circle.fill")
                    .foregroundStyle(.orange)
            }
            else if !snapshot.summaryText.isEmpty {
                Text(snapshot.summaryText)
                    .foregroundStyle(.secondary)
            }
        }
        .font(compact ? .caption : nil)
    }
}

/// The "waiting in the download queue" status, shown on a queued row and on the
/// album-detail control for a release that hasn't started downloading yet.
struct WaitingToDownloadLabel: View {
    var body: some View {
        Label("Waiting to download", systemImage: "clock")
            .font(.caption)
            .foregroundStyle(.secondary)
    }
}

/// One download-queue row: album title, file count and size, and the state —
/// a waiting label, the live progress bar, or the failure message.
private struct DownloadQueueRow: View {
    let op: BridgeDownloadOp

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(op.title)
                .font(.body)
                .lineLimit(1)
            Text(op.detailText)
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.secondary)
            stateView
        }
        .padding(.vertical, 4)
    }

    @ViewBuilder
    private var stateView: some View {
        switch op.state {
        case .queued:
            WaitingToDownloadLabel()
        case .active(let progress):
            DownloadTransferProgressView(progress: progress)
        case .failed(let error):
            Text(error)
                .font(.caption2)
                .foregroundStyle(.red)
        }
    }
}
