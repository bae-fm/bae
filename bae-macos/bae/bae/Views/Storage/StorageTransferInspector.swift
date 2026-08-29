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
        case download(operation: BridgeDownloadOp)
        case output(operation: BridgeOutputOp)
        case upload(group: BridgeUploadReleaseGroup)

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
                .download(operation: $0)
            }
        selectedItems += outputs.outputs
            .filter { $0.releaseId == releaseId }
            .map {
                .output(operation: $0)
            }
        selectedItems += outbox.uploadGroups
            .filter { $0.releaseId == releaseId }
            .map {
                .upload(group: $0)
            }
        items = selectedItems
    }
}

/// The trailing detail pane for one selected release's active transfers. The
/// item list is limited to records whose authoritative release id matches the
/// selection.
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
    @Binding
    var selection: Set<String>

    static func releaseId(in selection: Set<String>) -> String? {
        guard selection.count == 1 else { return nil }
        return selection.first
    }

    static func close(selection: inout Set<String>) {
        selection.removeAll()
    }

    var body: some View {
        let content = StorageTransferInspectorContent(
            releaseId: releaseId,
            downloads: downloadStore.snapshot,
            outputs: outputStore.snapshot,
            outbox: outboxStore.snapshot
        )
        VStack(spacing: 0) {
            header
            Divider()
            if content.items.isEmpty {
                ContentUnavailableView(
                    "Nothing here yet",
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
        .frame(minWidth: 360, idealWidth: 440, maxWidth: 520)
    }

    private var header: some View {
        HStack {
            Text("Transfers")
                .font(.headline)
            Spacer()
            Button {
                Self.close(selection: &selection)
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .help("Close")
        }
        .padding(.horizontal)
        .padding(.vertical, 10)
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
            }
            .foregroundStyle(.secondary)
            .padding(.horizontal)
            .padding(.vertical, 8)
            Divider()

            switch item {
            case .download(let operation):
                DownloadRow(op: operation) {
                    downloads.cancelDownload(operation.releaseId)
                }
            case .output(let operation):
                OutputRow(op: operation) {
                    outputs.cancelOutput(operation.releaseId)
                }
            case .upload(let group):
                OutboxReleaseRow(group: group) {
                    cancelTransition(group.releaseId)
                }
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
