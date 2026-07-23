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
            .safeAreaInset(edge: .bottom) {
                DownloadConcurrencyControl()
            }
        }
    }

    @ViewBuilder
    private func header(_ snapshot: BridgeDownloadSnapshot) -> some View {
        DownloadQueueSummaryLine(snapshot: snapshot, compact: false)
    }
}

/// Always-visible bottom control for the device-local download-concurrency
/// setting: how many downloads a pin fetches at once. Sits in a `safeAreaInset`
/// so it stays reachable whether the queue has entries or shows the empty
/// state. Mobile has no upload control — the app makes no uploads.
private struct DownloadConcurrencyControl: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Downloads.self)
    private var downloads

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Simultaneous downloads")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            TransferConcurrencyPicker(
                title: "Simultaneous downloads",
                value: configStore.config.maxConcurrentDownloads,
                setValue: downloads.setMaxConcurrentDownloads,
                showError: { configStore.showError($0) }
            )
            .labelsHidden()
        }
        .padding(.horizontal)
        .padding(.vertical, 10)
        .background(.bar)
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

#if DEBUG
#Preview {
    DownloadsView()
        .previewStores(
            downloadStore: DownloadStore(
                snapshot: PreviewData.downloadSnapshot(
                    queued: 1,
                    ops: [PreviewData.queuedDownloadOp]
                )
            )
        )
}
#endif
