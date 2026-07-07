import AppKit
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
                StorageTableView(
                    list: list,
                    selection: $selection,
                    sort: $sort,
                    libraryStore: libraryStore,
                    library: library,
                    runner: runner,
                )

                Divider()
                StorageFooter(list: list, libraryStore: libraryStore)
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
        rebuildTask = Task {
            await newList.loadInitial()
            guard !Task.isCancelled else {
                return
            }
            listRegistration = projectionRegistry.registerList(
                newList,
                domain: .albumList
            )
            list = newList
        }
    }
}

// MARK: - Footer

private struct StorageFooter: View {
    let list: StorageList
    let libraryStore: LibraryStore

    /// Sum of sizes over rows whose ids are populated and whose release
    /// is present in the store. Under-counts while pages are still
    /// loading — this is a visible-data footer, not a correctness
    /// contract. Pages fill in as the user scrolls.
    private var loadedTotalSize: Int64 {
        list.allLoadedIds.reduce(Int64(0)) { acc, id in
            guard let release = libraryStore.releaseSummaries[id] else {
                return acc
            }
            return acc + release.totalSize
        }
    }

    var body: some View {
        HStack {
            Text("\(list.totalCount) releases")
                .foregroundStyle(.secondary)
            Spacer()
            Text(
                "Total: \(ByteCountFormatter.string(fromByteCount: loadedTotalSize, countStyle: .file))"
            )
            .foregroundStyle(.secondary)
        }
        .font(.callout)
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}

// MARK: - Download (pin) queue

/// Bottom-pane section showing the in-memory download (pin) queue: a header
/// band with the summary, a pause/resume toggle, and a "Retry now" action, plus
/// one row per release (title, file count, size, a Queued/Active/Failed
/// badge, and a cancel button). Hidden when the queue is idle. Reads
/// `DownloadStore` at the leaf; the reducer is the sole writer, so actions don't
/// optimistically mutate — the follow-up `downloadQueueChanged` event refreshes
/// the section.
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
                header(snapshot)
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

    private func header(_ snapshot: BridgeDownloadSnapshot) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "arrow.down.circle")
                .foregroundStyle(.secondary)
            Text("Downloads").font(.callout.weight(.medium))
            if snapshot.paused {
                Label("Paused", systemImage: "pause.circle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
            }
            else if !snapshot.summaryText.isEmpty {
                Text(snapshot.summaryText)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(snapshot.paused ? "Resume" : "Pause") {
                downloads.setDownloadsPaused(!snapshot.paused)
            }
            Button("Retry now") {
                downloads.retryDownloads()
            }
            .disabled(snapshot.total.failed == 0)
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}

/// One download-queue row: album title, file count, size, a state badge, and a
/// cancel button. Mirrors `OutboxUploadRow`'s layout.
private struct DownloadRow: View {
    let op: BridgeDownloadOp
    let onCancel: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "arrow.down.circle")
                .foregroundStyle(.secondary)
                .frame(width: 16)

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

            Spacer()

            stateBadge
                .font(.caption)
                .frame(width: 130, alignment: .leading)

            Text(queuedRelativeLabel(op.createdAt))
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 90, alignment: .trailing)

            Button(action: onCancel) {
                Image(systemName: "xmark.circle")
            }
            .buttonStyle(.plain)
            .help("Cancel this download")
        }
        .padding(.horizontal)
        .padding(.vertical, 6)
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
/// idle. Reads `ExportStore` at the leaf; the reducer is the sole writer, so
/// actions don't optimistically mutate — the follow-up `exportQueueChanged`
/// event refreshes the section. Mirrors `DownloadsSection`.
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
                header(snapshot)
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

    private func header(_ snapshot: BridgeExportSnapshot) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "square.and.arrow.up")
                .foregroundStyle(.secondary)
            Text("Exporting").font(.callout.weight(.medium))
            if snapshot.paused {
                Label("Paused", systemImage: "pause.circle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
            }
            else if !snapshot.summaryText.isEmpty {
                Text(snapshot.summaryText)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(snapshot.paused ? "Resume" : "Pause") {
                exports.setExportsPaused(!snapshot.paused)
            }
            Button("Retry now") {
                exports.retryExports()
            }
            .disabled(snapshot.total.failed == 0)
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}

/// One export-queue row: album title, file count, size, a state badge (Queued /
/// Exporting at a percent / Failed with the reason in a tooltip), and a cancel
/// button. Mirrors `DownloadRow`.
private struct ExportRow: View {
    let op: BridgeExportOp
    let onCancel: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "square.and.arrow.up")
                .foregroundStyle(.secondary)
                .frame(width: 16)

            Text(op.title)
                .lineLimit(1)

            Text("\(op.fileCount) files · \(op.totalSizeText)")
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.secondary)

            Spacer()

            stateBadge
                .font(.caption)
                .frame(width: 130, alignment: .leading)

            Text(queuedRelativeLabel(op.createdAt))
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 90, alignment: .trailing)

            Button(action: onCancel) {
                Image(systemName: "xmark.circle")
            }
            .buttonStyle(.plain)
            .help("Cancel this export")
        }
        .padding(.horizontal)
        .padding(.vertical, 6)
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
/// its header. Reads `OutboxStore` at the leaf; the reducer is the sole writer,
/// so actions don't optimistically mutate — the follow-up `outboxChanged` event
/// refreshes the panel.
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
                header(snapshot)
                if !collapsed {
                    if snapshot.activeBytesTotal > 0 {
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

    private func header(_ snapshot: BridgeOutboxSnapshot) -> some View {
        HStack(spacing: 8) {
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

            Image(systemName: "arrow.up.arrow.down.circle")
                .foregroundStyle(.secondary)
            Text("Sync queue").font(.callout.weight(.medium))
            if snapshot.paused {
                Label("Paused", systemImage: "pause.circle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
            }
            else if !snapshot.summaryText.isEmpty {
                Text(snapshot.summaryText)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(snapshot.paused ? "Resume" : "Pause") {
                Task { await sync.setSyncPaused(!snapshot.paused) }
            }
            Button("Retry now") {
                Task {
                    do { try await sync.retryOutbox() }
                    catch {
                        uiStore.showError(
                            String(
                                localized:
                                    "Failed to retry uploads: \(error.localizedDescription)"
                            )
                        )
                    }
                }
            }
            .disabled(snapshot.total.failed == 0)
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
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
                            "Failed to cancel: \(error.localizedDescription)"
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
                            "Failed to cancel: \(error.localizedDescription)"
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
            ProgressView(value: snapshot.transferFraction)
                .progressViewStyle(.linear)
                // Dim the bar while paused so the visual matches the "Paused"
                // chip in the band above — paused-but-mid-progress reads as
                // active otherwise.
                .opacity(snapshot.paused ? 0.4 : 1)
            HStack(spacing: 8) {
                Text(snapshot.transferProgressText)
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

/// One queue-pane row per release (matching the storage table): the release
/// title, its file count and total size, and an aggregate state badge.
/// Right-click to cancel the release's transition. The orphaned-files bucket
/// (no release id) renders without a cancel — there's no release to act on.
private struct OutboxReleaseRow: View {
    let group: BridgeUploadReleaseGroup
    let onCancel: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "arrow.up.circle")
                .foregroundStyle(.secondary)
                .frame(width: 16)

            Text(group.displayTitle)
                .lineLimit(1)

            Text("\(Int(group.fileCount)) files · \(sizeText)")
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.secondary)

            Spacer()

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
    }

    private var sizeText: String {
        Int64(group.progress.bytesTotal).formatted(.byteCount(style: .file))
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

private struct OutboxDeleteRow: View {
    let op: BridgeDeleteOp
    let onCancel: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "trash")
                .foregroundStyle(.secondary)
                .frame(width: 16)

            Text(op.cloudKey)
                .lineLimit(1)

            Spacer()

            Label("Pending delete", systemImage: "clock")
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 130, alignment: .leading)

            Text(queuedRelativeLabel(op.createdAt))
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 90, alignment: .trailing)

            Button(action: onCancel) {
                Image(systemName: "xmark.circle")
            }
            .buttonStyle(.plain)
            .help("Remove from the sync queue")
        }
        .padding(.horizontal)
        .padding(.vertical, 6)
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
