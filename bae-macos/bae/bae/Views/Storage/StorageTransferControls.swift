import BaeKit
import SwiftUI

/// The selected release's active transfers, as the inspector read them.
struct StorageTransferControls: View {
    @Environment(DownloadStore.self)
    private var downloadStore
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(OutputStore.self)
    private var outputStore
    @Environment(Downloads.self)
    private var downloads
    @Environment(Outputs.self)
    private var outputs
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    let items: [BridgeStorageInspectorTransfer]

    var body: some View {
        VStack(spacing: 0) {
            ForEach(items, id: \.queueId) { item in
                section(item)
                Divider()
            }
        }
    }

    @ViewBuilder
    private func section(_ item: BridgeStorageInspectorTransfer)
        -> some View
    {
        VStack(spacing: 0) {
            HStack {
                Label(item.title, systemImage: item.icon)
                    .themeText(.heading)
                Spacer()
                let paused = item.pauseRequested
                Button(paused ? "Resume" : "Pause") {
                    setPaused(!paused, for: item)
                }
                .controlSize(.small)
                // Uploads have no queue-wide cancel; each release cancels its
                // own. Hidden rather than removed so every header lines up.
                Button("Cancel All", role: .destructive) {
                    cancelAll(item)
                }
                .controlSize(.small)
                .help(item.cancelAllHelp.map { Text($0) } ?? Text(verbatim: ""))
                .opacity(item.cancelAllHelp == nil ? 0 : 1)
                .allowsHitTesting(item.cancelAllHelp != nil)
                .accessibilityHidden(item.cancelAllHelp == nil)
            }
            .foregroundStyle(.secondary)
            .padding(.horizontal)
            .padding(.vertical, ThemeSpace.related)
            Divider()

            switch item {
            case .download(let operation, _):
                // The row alone reads the bytes the download moves.
                DownloadRow(
                    op: operation,
                    transfer: downloadStore.transfer(
                        forRelease: operation.releaseId
                    )
                ) {
                    run {
                        try await downloads.cancelDownload(operation.releaseId)
                    }
                }
            case .output(let operation, _):
                // The row alone reads the percent the export reaches.
                OutputRow(
                    op: operation,
                    transfer: outputStore.transfer(
                        forRelease: operation.releaseId
                    )
                ) {
                    run { try await outputs.cancelOutput(operation.releaseId) }
                }
            case .upload(let group, _):
                StorageUploadSummary(
                    group: group,
                    transfer: outboxStore.transfer(forRelease: group.releaseId)
                ) {
                    cancelTransition(group.releaseId)
                }
            }
        }
    }

    private func setPaused(
        _ paused: Bool,
        for item: BridgeStorageInspectorTransfer
    ) {
        switch item {
        case .download:
            run { try await downloads.setDownloadsPaused(paused) }
        case .output:
            run { try await outputs.setOutputsPaused(paused) }
        case .upload:
            run { try await sync.setSyncPaused(paused) }
        }
    }

    /// Cancel the whole queue this section belongs to, not only the selected
    /// release's entry in it.
    private func cancelAll(_ item: BridgeStorageInspectorTransfer) {
        switch item {
        case .download:
            run { try await downloads.cancelAllDownloads() }
        case .output:
            run { try await outputs.cancelAllOutputs() }
        case .upload:
            preconditionFailure("uploads have no queue-wide cancel")
        }
    }

    /// Run a queue command, surfacing its failure in the Storage Manager
    /// window.
    private func run(_ command: @escaping @Sendable () async throws -> Void) {
        Task {
            do { try await command() }
            catch { uiStore.showError(error) }
        }
    }

    /// Cancel a selected release's in-progress cloud transition, surfacing any
    /// failure in the Storage Manager window.
    private func cancelTransition(_ releaseId: String) {
        run { try await sync.cancelReleaseTransition(releaseId) }
    }
}
