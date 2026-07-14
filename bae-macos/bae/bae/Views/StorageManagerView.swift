import AppKit
import BaeKit
import SwiftUI

struct StorageManagerView: View {
    @Environment(Library.self)
    var library
    @Environment(LibraryStore.self)
    var libraryStore
    @Environment(ReleaseEditor.self)
    private var releaseEditor
    @Environment(Sync.self)
    private var sync
    @Environment(Downloads.self)
    private var downloads
    @Environment(Exports.self)
    private var exports
    @Environment(ConfigStore.self)
    private var configStore
    // OutboxSection / DownloadsSection / ExportingSection read their stores and
    // services at the leaf; uiStore is read here to surface outbox action errors
    // in this window (it's a separate scene from MainAppView, which owns the
    // other error alert).
    @Environment(UiStore.self)
    private var uiStore
    @Environment(ProjectionRegistry.self)
    private var projectionRegistry

    @State
    private var filter: BridgeStorageFilter = .all
    @State
    private var sort = BridgeStorageSort(
        field: .albumTitle,
        direction: .ascending
    )
    @State
    private var selection: Set<String> = []
    @State
    private var list: StorageList?
    @State
    private var listRegistration: ProjectionRegistration?
    /// Sum of `total_size` over every row the current filter matches — the
    /// full-universe figure the core aggregate computes, not just the pages
    /// loaded so far. `nil` until the first fetch for the current filter
    /// resolves; the footer renders nothing while it's `nil` rather than a
    /// zero/partial stand-in.
    @State
    private var totalSize: UInt64?
    @State
    private var totalSizeRegistration: ProjectionRegistration?
    @State
    private var rebuildTask: Task<Void, Never>?
    /// Runs row context-menu transitions; built lazily once the services are
    /// available from the environment.
    @State
    private var runner: StorageActionRunner?

    var body: some View {
        VStack(spacing: 0) {
            Picker("Filter", selection: $filter) {
                Text("All").tag(BridgeStorageFilter.all)
                Text("Unmanaged").tag(BridgeStorageFilter.local)
                Text("Managed").tag(BridgeStorageFilter.remote)
                Text("Uploading").tag(BridgeStorageFilter.uploading)
            }
            .pickerStyle(.segmented)
            .padding()

            if let list, let runner {
                if let error = list.initialLoadError {
                    LoadFailureView(line: error.line) {
                        Task { await list.loadInitial() }
                    }
                }
                else {
                    StorageTableView(
                        list: list,
                        selection: $selection,
                        sort: $sort,
                        libraryStore: libraryStore,
                        library: library,
                        runner: runner,
                    )

                    Divider()
                    StorageFooter(list: list, totalSize: totalSize)
                }
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }

            DownloadsSection()
            ExportingSection()
            OutboxSection()
        }
        .frame(minWidth: 700, minHeight: 400)
        .onAppear {
            if runner == nil {
                runner = StorageActionRunner(
                    releaseEditor: releaseEditor,
                    sync: sync,
                    downloads: downloads,
                    exports: exports,
                    configStore: configStore,
                    uiStore: uiStore,
                )
            }
        }
        .sheet(
            isPresented: Binding(
                get: { runner?.pendingManage != nil },
                set: { if !$0 { runner?.cancelManage() } },
            )
        ) {
            if let runner {
                ManageConfirmSheet(
                    onConfirm: { pin in runner.confirmManage(pin: pin) },
                    onCancel: { runner.cancelManage() },
                )
                .frame(width: 420)
            }
        }
        .errorAlert(uiStore)
        .task { rebuildList() }
        .onChange(of: filter) { _, _ in
            // Selection is scoped to the visible tab; switching tabs would
            // otherwise carry releases from the old filter into the new tab's
            // multi-select actions.
            selection = []
            rebuildList()
        }
        .onChange(of: sort) { _, _ in rebuildList() }
        .onDisappear {
            rebuildTask?.cancel()
        }
    }

    private func rebuildList() {
        rebuildTask?.cancel()
        let filter = filter
        let newList = StorageList(
            pageSource: StoragePageSource(
                library: library,
                sort: sort,
                filter: filter,
            ),
            ingest: { [libraryStore] rows in
                for row in rows {
                    _ = libraryStore.internAlbumSummary(row.album)
                    _ = libraryStore.internReleaseSummary(row.release)
                }
            },
            onError: { [uiStore] error in
                uiStore.showError(error)
            },
        )
        // The total-size figure is scoped to the same filter as the list and
        // refreshes on the same `.albumList` invalidations that reload the
        // list's rows, via its own `Projection` rather than piggybacking on
        // `PaginatedList.invalidate()` (which only re-fetches the row count).
        let totalSizeProjection = Projection<UInt64>(
            domain: .albumList,
            query: { [library] _ in
                try await library.storageTotalSize(filter)
            },
            apply: { totalSize = $0 },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
        rebuildTask = Task {
            async let totalSizeResult = library.storageTotalSize(filter)
            await newList.loadInitial()
            guard !Task.isCancelled else {
                return
            }
            listRegistration = projectionRegistry.registerList(
                newList,
                domain: .albumList
            )
            totalSizeRegistration = projectionRegistry.register(
                totalSizeProjection
            )
            list = newList
            do {
                totalSize = try await totalSizeResult
            }
            catch {
                totalSize = nil
                uiStore.showError(
                    DisplayError(error)
                )
            }
        }
    }
}

// MARK: - Footer

private struct StorageFooter: View {
    let list: StorageList
    /// The core aggregate over every row the current filter matches. `nil`
    /// until fetched — rendered as absence, not a zero/partial stand-in.
    let totalSize: UInt64?

    var body: some View {
        HStack {
            Text("\(list.totalCount) releases")
                .foregroundStyle(.secondary)
            Spacer()
            if let totalSize {
                Text(
                    "Total: \(ByteCountFormatter.string(fromByteCount: Int64(totalSize), countStyle: .file))"
                )
                .foregroundStyle(.secondary)
            }
        }
        .font(.callout)
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}

// MARK: - Queue pane scaffolding

/// Header band shared by the three queue panes: optional leading slot
/// (the outbox pane's collapse chevron), icon, title, paused chip or
/// summary, pause/resume, and retry.
private struct QueueSectionHeader<Leading: View>: View {
    let icon: String
    let title: LocalizedStringKey
    let paused: Bool
    let summaryText: String
    let retryDisabled: Bool
    let onSetPaused: (Bool) -> Void
    let onRetry: () -> Void
    @ViewBuilder
    let leading: () -> Leading

    var body: some View {
        HStack(spacing: 8) {
            leading()
            Image(systemName: icon)
                .foregroundStyle(.secondary)
            Text(title).font(.callout.weight(.medium))
            if paused {
                Label("Paused", systemImage: "pause.circle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
            }
            else if !summaryText.isEmpty {
                Text(summaryText)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(paused ? "Resume" : "Pause") { onSetPaused(!paused) }
            Button("Retry now", action: onRetry)
                .disabled(retryDisabled)
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}

extension QueueSectionHeader where Leading == EmptyView {
    init(
        icon: String,
        title: LocalizedStringKey,
        paused: Bool,
        summaryText: String,
        retryDisabled: Bool,
        onSetPaused: @escaping (Bool) -> Void,
        onRetry: @escaping () -> Void
    ) {
        self.init(
            icon: icon,
            title: title,
            paused: paused,
            summaryText: summaryText,
            retryDisabled: retryDisabled,
            onSetPaused: onSetPaused,
            onRetry: onRetry
        ) { EmptyView() }
    }
}

/// Row scaffolding shared by the download, export, and outbox-delete queue
/// rows: leading icon, caller content, then the trailing state badge /
/// queued-time / cancel cluster.
private struct QueueRow<Content: View, Badge: View>: View {
    let icon: String
    let createdAt: Int64
    let cancelHelp: LocalizedStringKey
    let onCancel: () -> Void
    @ViewBuilder
    let content: () -> Content
    @ViewBuilder
    let badge: () -> Badge

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .foregroundStyle(.secondary)
                .frame(width: 16)

            content()

            Spacer()

            badge()
                .font(.caption)
                .frame(width: 130, alignment: .leading)

            Text(queuedRelativeLabel(createdAt))
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 90, alignment: .trailing)

            Button(action: onCancel) {
                Image(systemName: "xmark.circle")
            }
            .buttonStyle(.plain)
            .help(cancelHelp)
        }
        .padding(.horizontal)
        .padding(.vertical, 6)
    }
}

// MARK: - Download (pin) queue

/// Bottom-pane section showing the in-memory download (pin) queue: a header
/// band with the summary, a pause/resume toggle, and a "Retry now" action, plus
/// one row per release (title, file count, size, a Queued/Active/Failed
/// badge, and a cancel button). Hidden when the queue is idle. Reads
/// `DownloadStore` at the leaf; the download projection is the sole writer, so
/// actions don't optimistically mutate — a `.downloadQueue` invalidation
/// refetches and refreshes the section.
private struct DownloadsSection: View {
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

/// One download-queue row: album title, file count, size, a state badge, and a
/// cancel button.
private struct DownloadRow: View {
    let op: BridgeDownloadOp
    let onCancel: () -> Void

    var body: some View {
        QueueRow(
            icon: "arrow.down.circle",
            createdAt: op.createdAt,
            cancelHelp: "Cancel this download",
            onCancel: onCancel
        ) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(op.title)
                        .lineLimit(1)

                    Text(op.detailText)
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                progressView
            }
        } badge: {
            stateBadge
        }
    }

    @ViewBuilder
    private var stateBadge: some View {
        switch op.state {
        case .queued:
            Label("Queued", systemImage: "clock")
                .foregroundStyle(.secondary)
        case .active:
            Label(
                "Downloading",
                systemImage: "arrow.down.circle.fill"
            )
            .foregroundStyle(.orange)
        case .failed(let error):
            Label("Failed", systemImage: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
                .help(error)
        }
    }

    @ViewBuilder
    private var progressView: some View {
        if case .active(let progress) = op.state {
            DownloadTransferProgressView(progress: progress)
        }
    }
}

// MARK: - Exporting queue

/// Pane showing the in-memory export queue: a header with the summary and
/// pause/retry controls, and a per-release row list. Hidden when the queue is
/// idle. Reads `ExportStore` at the leaf; the export projection is the sole
/// writer, so actions don't optimistically mutate — an `.exportQueue`
/// invalidation refetches and refreshes the section. Mirrors
/// `DownloadsSection`.
private struct ExportingSection: View {
    @Environment(ExportStore.self)
    private var exportStore
    @Environment(Exports.self)
    private var exports

    var body: some View {
        let snapshot = exportStore.snapshot
        if !snapshot.exports.isEmpty {
            Divider()
            VStack(spacing: 0) {
                QueueSectionHeader(
                    icon: "square.and.arrow.up",
                    title: "Exporting",
                    paused: snapshot.paused,
                    summaryText: snapshot.summaryText,
                    retryDisabled: snapshot.total.failed == 0,
                    onSetPaused: { exports.setExportsPaused($0) },
                    onRetry: { exports.retryExports() }
                )
                Divider()
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(snapshot.exports, id: \.releaseId) { op in
                            ExportRow(op: op) {
                                exports.cancelExport(op.releaseId)
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

/// One export-queue row: album title, file count, size, a state badge (Queued /
/// Exporting at a percent / Failed with the reason in a tooltip), and a cancel
/// button.
private struct ExportRow: View {
    let op: BridgeExportOp
    let onCancel: () -> Void

    var body: some View {
        QueueRow(
            icon: "square.and.arrow.up",
            createdAt: op.createdAt,
            cancelHelp: "Cancel this export",
            onCancel: onCancel
        ) {
            Text(op.title)
                .lineLimit(1)

            Text("\(op.fileCount) files · \(op.totalSizeText)")
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.secondary)
        } badge: {
            stateBadge
        }
    }

    @ViewBuilder
    private var stateBadge: some View {
        switch op.state {
        case .queued:
            Label("Queued", systemImage: "clock")
                .foregroundStyle(.secondary)
        case .active(let percent):
            Label(
                "Exporting \(Int(percent))%",
                systemImage: "square.and.arrow.up.fill"
            )
            .foregroundStyle(.orange)
        case .failed(let error):
            Label("Failed", systemImage: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
                .help(error)
        }
    }
}

// MARK: - Outbox processing queue

/// Bottom pane showing the cloud outbox processing queue: a master progress
/// row, a summary band with a "Retry now" action, and per-item lists for
/// uploads and deletes. Hidden when the queue is idle. The user can drag the
/// top edge to resize the item list (persisted) and collapse the pane to just
/// its header. Reads `OutboxStore` at the leaf; the outbox projection is the
/// sole writer, so actions don't optimistically mutate — an `.outbox`
/// invalidation refetches and refreshes the panel.
private struct OutboxSection: View {
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    /// Persisted item-list height and collapsed state, so the pane keeps its
    /// size and visibility across sessions.
    @AppStorage("storageQueuePaneHeight")
    private var storedHeight: Double = 180
    @AppStorage("storageQueuePaneCollapsed")
    private var collapsed: Bool = false
    /// Live drag delta while the resize handle is held; resets to 0 on release.
    @GestureState
    private var dragOffset: CGFloat = 0

    /// Bounds for the resizable item list.
    private let heightRange: ClosedRange<CGFloat> = 80...480

    /// The item-list height with a drag `offset` applied, clamped to
    /// `heightRange`. Dragging the handle up (negative translation) grows the
    /// list; down shrinks it. Used for both the live height and the committed
    /// height so the two can't drift.
    private func paneHeight(applying offset: CGFloat) -> CGFloat {
        let height = CGFloat(storedHeight) - offset
        return min(max(height, heightRange.lowerBound), heightRange.upperBound)
    }

    var body: some View {
        let snapshot = outboxStore.snapshot
        if !snapshot.uploadGroups.isEmpty || !snapshot.deletes.isEmpty {
            Divider()
            VStack(spacing: 0) {
                if !collapsed {
                    resizeHandle
                }
                QueueSectionHeader(
                    icon: "arrow.up.arrow.down.circle",
                    title: "Sync queue",
                    paused: snapshot.paused,
                    summaryText: snapshot.summaryText,
                    retryDisabled: snapshot.total.failed == 0,
                    onSetPaused: { paused in
                        Task { await sync.setSyncPaused(paused) }
                    },
                    onRetry: {
                        Task {
                            do { try await sync.retryOutbox() }
                            catch {
                                uiStore.showError(
                                    String(
                                        localized:
                                            "Failed to retry uploads: \(error.displayLine)"
                                    )
                                )
                            }
                        }
                    },
                    leading: { collapseButton }
                )
                if !collapsed {
                    if snapshot.total.bytesTotal > 0 {
                        OutboxTotalProgress(snapshot: snapshot)
                    }
                    Divider()
                    itemList(snapshot)
                        .frame(height: paneHeight(applying: dragOffset))
                }
            }
        }
    }

    /// A thin strip the user drags to resize the queue. Shows the resize cursor
    /// on hover; commits the new height on release.
    private var resizeHandle: some View {
        Color.clear
            .frame(height: 6)
            .overlay(Divider())
            .contentShape(Rectangle())
            .onHover { inside in
                if inside {
                    NSCursor.resizeUpDown.push()
                }
                else {
                    NSCursor.pop()
                }
            }
            .gesture(
                DragGesture()
                    .updating($dragOffset) { value, state, _ in
                        state = value.translation.height
                    }
                    .onEnded { value in
                        storedHeight = Double(
                            paneHeight(applying: value.translation.height)
                        )
                    }
            )
    }

    /// The collapse/expand chevron shown in the sync-queue header's leading
    /// slot; toggles the pane between its full body and just the header.
    private var collapseButton: some View {
        Button {
            collapsed.toggle()
        } label: {
            Image(systemName: collapsed ? "chevron.right" : "chevron.down")
                .foregroundStyle(.secondary)
        }
        .buttonStyle(.plain)
        .help(
            collapsed ? "Expand the sync queue" : "Collapse the sync queue"
        )
    }

    private func itemList(_ snapshot: BridgeOutboxSnapshot) -> some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                // Uploads are grouped per release (matching the storage table);
                // right-click a release to cancel its transition.
                ForEach(snapshot.uploadGroups, id: \.releaseId) { group in
                    OutboxReleaseRow(group: group) {
                        if let releaseId = group.releaseId {
                            cancelTransition(releaseId)
                        }
                    }
                    Divider()
                }
                // Deletes stay per-file — a delete is a single-file operation.
                ForEach(snapshot.deletes, id: \.id) { op in
                    OutboxDeleteRow(op: op) { cancel(op.id) }
                    Divider()
                }
            }
        }
    }

    /// Cancel a release's in-progress transition, surfacing any failure.
    private func cancelTransition(_ releaseId: String) {
        Task {
            do { try await sync.cancelReleaseTransition(releaseId) }
            catch {
                uiStore.showError(
                    String(
                        localized:
                            "Failed to cancel: \(error.displayLine)"
                    )
                )
            }
        }
    }

    /// Remove a queued delete, surfacing any failure.
    private func cancel(_ id: Int64) {
        Task {
            do { try await sync.cancelOutboxItem(id) }
            catch {
                uiStore.showError(
                    String(
                        localized:
                            "Failed to cancel: \(error.displayLine)"
                    )
                )
            }
        }
    }
}

/// Master progress strip: filled progress bar + bytes done/total, throughput,
/// and ETA. All three labels are pre-formatted by core.
private struct OutboxTotalProgress: View {
    let snapshot: BridgeOutboxSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ProgressView(value: snapshot.total.fraction)
                .progressViewStyle(.linear)
                // Dim the bar while paused so the visual matches the "Paused"
                // chip in the band above — paused-but-mid-progress reads as
                // active otherwise.
                .opacity(snapshot.paused ? 0.4 : 1)
            HStack(spacing: 8) {
                Text(snapshot.total.bytesText)
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                if !snapshot.throughputText.isEmpty {
                    Text(verbatim: "·").foregroundStyle(.tertiary)
                    Text(snapshot.throughputText)
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                if !snapshot.etaText.isEmpty {
                    Text(verbatim: "·").foregroundStyle(.tertiary)
                    Text(snapshot.etaText)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 4)
    }
}

/// One expandable queue-pane row per release (matching the storage table):
/// the release title, its file count and cumulative byte progress, a
/// determinate progress bar, and an aggregate state badge. Expanded (the
/// default), it lists every file with its own state and live progress.
/// Right-click to cancel the release's transition. The orphaned-files bucket
/// (no release id) renders without a cancel — there's no release to act on.
private struct OutboxReleaseRow: View {
    let group: BridgeUploadReleaseGroup
    let onCancel: () -> Void

    /// Per-row disclosure, keyed by the `ForEach` identity (the release id).
    /// Expanded by default: the per-file list is the pane's point.
    @State
    private var expanded = true

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Button {
                    expanded.toggle()
                } label: {
                    Image(
                        systemName: expanded ? "chevron.down" : "chevron.right"
                    )
                    .foregroundStyle(.secondary)
                    .frame(width: 16)
                }
                .buttonStyle(.plain)
                .help(expanded ? "Hide files" : "Show files")

                Text(group.displayTitle)
                    .lineLimit(1)

                Text("\(group.files.count) files · \(group.progress.bytesText)")
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)

                Spacer()

                ProgressView(value: group.progress.fraction)
                    .progressViewStyle(.linear)
                    .frame(width: 140)

                stateBadge
                    .font(.caption)
                    .frame(width: 130, alignment: .leading)
            }
            .padding(.horizontal)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
            .contextMenu {
                if group.releaseId != nil {
                    Button("Cancel", role: .destructive, action: onCancel)
                }
            }
            if expanded {
                ForEach(group.files, id: \.fileId) { file in
                    OutboxFileRow(file: file)
                }
            }
        }
    }

    @ViewBuilder
    private var stateBadge: some View {
        let remaining = Int(group.progress.pending)
        switch group.progress.activity {
        case .queued:
            Label("Queued (\(remaining))", systemImage: "clock")
                .foregroundStyle(.secondary)
        case .uploading:
            Label(
                "Uploading (\(remaining))",
                systemImage: "arrow.up.circle.fill"
            )
            .foregroundStyle(.orange)
        case .retrying:
            Label(
                "Retrying (\(remaining))",
                systemImage: "exclamationmark.triangle.fill"
            )
            .foregroundStyle(.red)
        case .none:
            EmptyView()
        }
    }
}

/// One file inside an expanded release row: state icon, name, and — while the
/// file transfers — a live determinate bar with its byte progress. Completed
/// files read as a checkmark with their size; a failed file carries its error
/// as a tooltip and waits for the next retry.
private struct OutboxFileRow: View {
    let file: BridgeUploadFileOp

    var body: some View {
        HStack(spacing: 12) {
            stateIcon
                .frame(width: 16)

            Text(file.displayName)
                .font(.caption)
                .lineLimit(1)
                .foregroundStyle(.secondary)

            Spacer()

            if file.state == .uploading {
                ProgressView(value: fraction)
                    .progressViewStyle(.linear)
                    .frame(width: 140)
            }

            Text(bytesText)
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.tertiary)
                .frame(width: 130, alignment: .leading)
        }
        .padding(.leading, 44)
        .padding(.trailing)
        .padding(.vertical, 3)
    }

    @ViewBuilder
    private var stateIcon: some View {
        switch file.state {
        case .queued:
            Image(systemName: "clock")
                .foregroundStyle(.secondary)
        case .uploading:
            Image(systemName: "arrow.up.circle.fill")
                .foregroundStyle(.orange)
        case .retrying:
            // The converter sets `lastError` for every retrying file; the
            // conditional only spares a hypothetical empty tooltip.
            let icon = Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
            if let error = file.lastError {
                icon.help(error)
            }
            else {
                icon
            }
        case .done:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.green)
        }
    }

    private var fraction: Double {
        guard file.bytesTotal > 0 else { return 0 }
        return Double(file.bytesDone) / Double(file.bytesTotal)
    }

    /// "6.2 MB of 12.4 MB" while transferring; just the size otherwise.
    private var bytesText: String {
        let total = Int64(file.bytesTotal).formatted(.byteCount(style: .file))
        guard file.state == .uploading else { return total }
        return String(
            format: QueueSummary.message("core.outbox.bytes_progress"),
            Int64(file.bytesDone).formatted(.byteCount(style: .file)),
            total
        )
    }
}

private struct OutboxDeleteRow: View {
    let op: BridgeDeleteOp
    let onCancel: () -> Void

    var body: some View {
        QueueRow(
            icon: "trash",
            createdAt: op.createdAt,
            cancelHelp: "Remove from the sync queue",
            onCancel: onCancel
        ) {
            Text(op.cloudKey)
                .lineLimit(1)
        } badge: {
            Label("Pending delete", systemImage: "clock")
                .foregroundStyle(.secondary)
        }
    }
}

/// Relative "queued" label (e.g. "2m ago") from an enqueue time in Unix epoch
/// milliseconds.
private func queuedRelativeLabel(_ epochMs: Int64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(epochMs) / 1000)
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .abbreviated
    return formatter.localizedString(for: date, relativeTo: Date())
}
