import BaeKit
import SwiftUI

/// Manages the download queue; the list redraws from the next queue snapshot
/// after each action, never optimistically.
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
                    // The queue can drain while the sheet is open, so show an
                    // empty state instead of dismissing.
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

/// This device's setting for how many downloads a pin fetches at once, shown
/// at the bottom even when the queue is empty.
private struct DownloadConcurrencyControl: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Downloads.self)
    private var downloads

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Simultaneous downloads")
                .themeText(.detail)
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

/// One download-queue row: title, file count and size, and its state.
private struct DownloadQueueRow: View {
    let op: BridgeDownloadOp

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(op.title)
                .themeText(.rowTitle)
                .lineLimit(1)
            Text(op.detailText)
                .monospacedDigit()
                .themeText(.detail)
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
                .themeText(.detail)
                .foregroundStyle(Theme.danger)
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
