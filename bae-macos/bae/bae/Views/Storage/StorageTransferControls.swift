import BaeKit
import SwiftUI

/// The selected release's active transfers. The item list is limited to records
/// whose authoritative release id matches the selection.
struct StorageTransferControls: View {
    @Environment(DownloadStore.self)
    private var downloadStore
    @Environment(OutputStore.self)
    private var outputStore
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(Downloads.self)
    private var downloads
    @Environment(Outputs.self)
    private var outputs
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    let releaseId: String
    var body: some View {
        let items = bridgeStorageInspectorTransfers(
            releaseId: releaseId,
            downloads: downloadStore.snapshot,
            outputs: outputStore.snapshot,
            outbox: outboxStore.snapshot
        )
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
                    .font(.callout.weight(.medium))
                Spacer()
                let paused = item.pauseRequested
                Button(paused ? "Resume" : "Pause") {
                    setPaused(!paused, for: item)
                }
                .controlSize(.small)
            }
            .foregroundStyle(.secondary)
            .padding(.horizontal)
            .padding(.vertical, 8)
            Divider()

            switch item {
            case .download(let operation, _):
                DownloadRow(op: operation) {
                    downloads.cancelDownload(operation.releaseId)
                }
            case .output(let operation, _):
                OutputRow(op: operation) {
                    outputs.cancelOutput(operation.releaseId)
                }
            case .upload(let group, _):
                StorageUploadSummary(group: group) {
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
            downloads.setDownloadsPaused(paused)
        case .output:
            outputs.setOutputsPaused(paused)
        case .upload:
            Task {
                do { try await sync.setSyncPaused(paused) }
                catch { uiStore.showError(error) }
            }
        }
    }

    /// Cancel a selected release's in-progress cloud transition, surfacing any
    /// failure in the Storage Manager window.
    private func cancelTransition(_ releaseId: String) {
        Task {
            do { try await sync.cancelReleaseTransition(releaseId) }
            catch {
                uiStore.showError(error)
            }
        }
    }
}
