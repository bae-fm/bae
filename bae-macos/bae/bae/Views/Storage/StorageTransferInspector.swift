import BaeKit
import SwiftUI

/// The active transfer records that belong in one release's inspector.
struct StorageTransferInspectorContent {
    enum ItemKind: Hashable {
        case download
        case output
        case upload
    }

    enum Item: Identifiable {
        case download(operation: BridgeDownloadOp, paused: Bool)
        case output(operation: BridgeOutputOp, paused: Bool)
        case upload(group: BridgeUploadReleaseGroup, paused: Bool)

        var id: ItemKind {
            switch self {
            case .download: .download
            case .output: .output
            case .upload: .upload
            }
        }

        var title: LocalizedStringKey {
            switch self {
            case .download: "Downloads"
            case .output: "Export & Save"
            case .upload: "Sync queue"
            }
        }

        var icon: String {
            switch self {
            case .download: "arrow.down.circle"
            case .output: "square.and.arrow.up"
            case .upload: "arrow.up.arrow.down.circle"
            }
        }

        var pauseRequested: Bool {
            switch self {
            case .download(_, let paused), .output(_, let paused),
                .upload(_, let paused):
                paused
            }
        }
    }

    let items: [Item]

    init(
        releaseId: String,
        downloads: BridgeDownloadSnapshot,
        outputs: BridgeOutputSnapshot,
        outbox: BridgeOutboxSnapshot
    ) {
        var selectedItems: [Item] = downloads.downloads
            .filter { $0.releaseId == releaseId }
            .map {
                .download(operation: $0, paused: downloads.paused)
            }
        selectedItems += outputs.outputs
            .filter { $0.releaseId == releaseId }
            .map {
                .output(operation: $0, paused: outputs.paused)
            }
        selectedItems += outbox.uploadGroups
            .filter { $0.releaseId == releaseId }
            .map {
                .upload(group: $0, paused: outbox.pauseRequested)
            }
        items = selectedItems
    }
}

/// The selected release's active transfers. The item list is limited to records
/// whose authoritative release id matches the selection.
struct StorageTransferInspector: View {
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
        let content = StorageTransferInspectorContent(
            releaseId: releaseId,
            downloads: downloadStore.snapshot,
            outputs: outputStore.snapshot,
            outbox: outboxStore.snapshot
        )
        Group {
            if content.items.isEmpty {
                ContentUnavailableView(
                    "No active transfers",
                    systemImage: "arrow.up.arrow.down.circle"
                )
            }
            else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(content.items) { item in
                            section(item)
                            Divider()
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder
    private func section(_ item: StorageTransferInspectorContent.Item)
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
                OutboxReleaseRow(group: group) {
                    cancelTransition(group.releaseId)
                }
            }
        }
    }

    private func setPaused(
        _ paused: Bool,
        for item: StorageTransferInspectorContent.Item
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
