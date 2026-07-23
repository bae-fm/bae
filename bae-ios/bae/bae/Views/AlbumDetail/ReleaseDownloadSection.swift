import BaeKit
import SwiftUI

/// Offline control for the shown release: Download / progress + Cancel /
/// Downloaded + Remove Download. Core joins the pin state, the storage actions
/// it offers, and the download queue into that state; the snapshot and the
/// release invalidations keep it live.
struct ReleaseDownloadSection: View {
    let releaseId: String
    let detail: ReleaseDetail

    @Environment(Downloads.self)
    private var downloads
    @Environment(DownloadStore.self)
    private var downloadStore

    @State
    private var unpinTask: Task<Void, Never>?
    @State
    private var unpinError: String?

    var body: some View {
        let status = bridgeReleaseDownloadStatus(
            pinned: detail.summary.pinned,
            storageActions: detail.storageActions,
            downloads: downloadStore.snapshot,
            releaseId: releaseId
        )
        VStack(alignment: .leading, spacing: 6) {
            control(status)
            if let unpinError {
                Text(unpinError)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
        .onDisappear { unpinTask?.cancel() }
    }

    @ViewBuilder
    private func control(_ status: BridgeReleaseDownloadStatus?) -> some View {
        switch status {
        case nil:
            EmptyView()
        case .available:
            // Fire-and-forget: progress and queue state arrive via the download
            // snapshot. Re-enqueuing is idempotent — core skips ids already
            // queued or pinned.
            actionButton("Download", systemImage: "arrow.down.circle") {
                Task { await downloads.queuePins([releaseId]) }
            }
        case .queued:
            HStack(spacing: 8) {
                WaitingToDownloadLabel()
                cancelButton
            }
        case .downloading(let progress):
            VStack(alignment: .leading, spacing: 6) {
                DownloadTransferProgressView(progress: progress)
                cancelButton
            }
        case .failed(let message):
            VStack(alignment: .leading, spacing: 6) {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(.red)
                HStack(spacing: 8) {
                    // Core has no per-item retry: `retryDownloads` flips every
                    // failed entry back to queued, like the macOS Downloads pane.
                    actionButton("Retry", systemImage: "arrow.clockwise") {
                        downloads.retryDownloads()
                    }
                    cancelButton
                }
            }
        case .downloaded:
            downloadedControl
        }
    }

    private var cancelButton: some View {
        actionButton("Cancel", systemImage: "xmark") {
            downloads.cancelDownload(releaseId)
        }
    }

    /// A bordered caption button — the shared shape for every download action.
    /// A `role` (e.g. `.destructive`) drops the accent tint so the role's own
    /// styling shows.
    private func actionButton(
        _ titleKey: LocalizedStringKey,
        systemImage: String,
        role: ButtonRole? = nil,
        action: @escaping () -> Void
    ) -> some View {
        Button(role: role, action: action) {
            Label(titleKey, systemImage: systemImage)
                .font(.caption)
        }
        .buttonStyle(.bordered)
        .tint(role == nil ? Theme.accent : nil)
    }

    @ViewBuilder
    private var downloadedControl: some View {
        HStack(spacing: 8) {
            Label("Downloaded", systemImage: "arrow.down.circle.fill")
                .font(.caption)
                .foregroundStyle(.secondary)
            if unpinTask != nil {
                ProgressView()
                    .controlSize(.small)
            }
            else {
                actionButton(
                    "Remove Download",
                    systemImage: "trash",
                    role: .destructive
                ) {
                    removeDownload()
                }
            }
        }
    }

    private func removeDownload() {
        unpinTask?.cancel()
        unpinError = nil
        unpinTask = Task {
            defer { unpinTask = nil }
            do {
                try await downloads.unpinRelease(releaseId)
            }
            catch is CancellationError {
                // View dismissed mid-unpin; core's drop guard emits the
                // terminal ReleaseTransferEnded, which refreshes the release.
            }
            catch {
                unpinError = error.displayLine
            }
        }
    }
}
