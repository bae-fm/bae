import BaeKit
import SwiftUI

/// Download, progress and Remove Download controls for the shown release, from
/// the status core derives.
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
                    .themeText(.detail)
                    .foregroundStyle(Theme.danger)
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
            // Progress arrives through the download snapshot; core skips
            // releases already queued or pinned.
            actionButton("Download", systemImage: "arrow.down.circle") {
                Task { try await downloads.queuePins([releaseId]) }
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
                    .themeText(.detail)
                    .foregroundStyle(Theme.danger)
                HStack(spacing: 8) {
                    // Core has no per-item retry; this requeues every failed
                    // download.
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

    /// A bordered button with a detail label; a `role` drops the accent tint so
    /// the role's own styling shows.
    private func actionButton(
        _ titleKey: LocalizedStringKey,
        systemImage: String,
        role: ButtonRole? = nil,
        action: @escaping () -> Void
    ) -> some View {
        Button(role: role, action: action) {
            Label(titleKey, systemImage: systemImage)
                .themeText(.detail)
        }
        .buttonStyle(.bordered)
        .tint(role == nil ? Theme.accent : nil)
    }

    @ViewBuilder
    private var downloadedControl: some View {
        HStack(spacing: 8) {
            Label("Downloaded", systemImage: "arrow.down.circle.fill")
                .themeText(.detail)
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
            catch is CancellationError {}
            catch {
                unpinError = error.displayLine
            }
        }
    }
}

#if DEBUG
#Preview {
    ReleaseDownloadSection(releaseId: "rel-a-1", detail: PreviewData.releaseDetail)
        .previewStores()
}
#endif
