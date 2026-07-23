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
    @Environment(Outputs.self)
    private var outputs
    @Environment(ConfigStore.self)
    private var configStore
    // OutboxSection / DownloadsSection / OutputSection read their stores and
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
            OutputSection()
            OutboxSection()
            Divider()
            TransferConcurrencyBar()
        }
        .frame(minWidth: 700, minHeight: 400)
        .onAppear {
            if runner == nil {
                runner = StorageActionRunner(
                    releaseEditor: releaseEditor,
                    sync: sync,
                    downloads: downloads,
                    outputs: outputs,
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
                uiStore.showError(error)
            }
        }
    }
}
