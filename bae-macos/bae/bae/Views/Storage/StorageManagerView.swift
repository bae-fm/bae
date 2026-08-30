import BaeKit
import SwiftUI

struct StorageManagerView: View {
    @Environment(Library.self)
    private var library
    @Environment(StorageManagerStore.self)
    private var storageManagerStore
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
    // The inspector reads transfer stores at the leaf; uiStore is read here to
    // surface transfer and storage-action errors in this window (it's a
    // separate scene from MainAppView, which owns the other error alert).
    @Environment(UiStore.self)
    private var uiStore

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
    private var inspectorPresented: Bool
    private let initialInspectorTab: StorageInspectorTab
    /// Runs row context-menu transitions; built lazily once the services are
    /// available from the environment.
    @State
    private var runner: StorageActionRunner?

    init(
        initialSelection: Set<String> = [],
        initialInspectorPresented: Bool = false,
        initialInspectorTab: StorageInspectorTab = .contents
    ) {
        _selection = State(initialValue: initialSelection)
        _inspectorPresented = State(
            initialValue: initialInspectorPresented
        )
        self.initialInspectorTab = initialInspectorTab
    }

    var body: some View {
        HSplitView {
            releaseList
                .frame(minWidth: 440, maxHeight: .infinity)

            if inspectorPresented {
                StorageInspector(
                    releaseId: StorageInspector.releaseId(in: selection),
                    isPresented: $inspectorPresented,
                    initialTab: initialInspectorTab
                )
            }
        }
        .frame(minWidth: 700, minHeight: 400)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Toggle(isOn: $inspectorPresented) {
                    Label("Inspector", systemImage: "sidebar.trailing")
                }
                .toggleStyle(.button)
            }
        }
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
                get: { runner?.pendingMoveToCloud != nil },
                set: { if !$0 { runner?.cancelMoveToCloud() } },
            )
        ) {
            if let runner {
                MoveToCloudConfirmSheet(
                    onConfirm: { pin in runner.confirmMoveToCloud(pin: pin) },
                    onCancel: { runner.cancelMoveToCloud() },
                )
                .frame(width: 420)
            }
        }
        .errorAlert(uiStore)
        .task { updateQuery() }
        .onChange(of: filter) { _, _ in
            // Selection is scoped to the visible tab; switching tabs would
            // otherwise carry releases from the old filter into the new tab's
            // multi-select actions.
            selection = []
            updateQuery()
        }
        .onChange(of: sort) { _, _ in updateQuery() }
        .onDisappear {
            storageManagerStore.cancel()
        }
    }

    private var releaseList: some View {
        VStack(spacing: 0) {
            Picker("Filter", selection: $filter) {
                Text("All").tag(BridgeStorageFilter.all)
                Text("Local").tag(BridgeStorageFilter.local)
                Text("Cloud").tag(BridgeStorageFilter.remote)
                Text("Sync queue").tag(BridgeStorageFilter.uploading)
            }
            .pickerStyle(.segmented)
            .padding()

            if let list = storageManagerStore.list, let runner {
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
                        inspectorPresented: $inspectorPresented,
                        sortingEnabled: filter != .uploading,
                        libraryStore: libraryStore,
                        runner: runner,
                    )
                    Divider()
                    StorageFooter(
                        list: list,
                        totalSize: storageManagerStore.totalSize
                    )
                }
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    private func updateQuery() {
        storageManagerStore.update(filter: filter, sort: sort)
    }
}

#if DEBUG
    @MainActor
    struct StorageManagerPreviewScene: View {
        let library: Library
        let libraryStore = LibraryStore()
        let uiStore = UiStore()
        let downloadStore: DownloadStore
        let outputStore: OutputStore
        let outboxStore: OutboxStore

        let initialSelection: Set<String>
        let initialInspectorPresented: Bool
        let initialInspectorTab: StorageInspectorTab

        init(
            rows: [BridgeStorageRow] = PreviewData.storageRows,
            selectedReleaseId: String? = nil,
            inspectorPresented: Bool = false,
            inspectorTab: StorageInspectorTab = .contents,
            downloadSnapshot: BridgeDownloadSnapshot =
                PreviewData
                .downloadSnapshot(),
            outputSnapshot: BridgeOutputSnapshot = PreviewData.outputSnapshot(),
            outboxSnapshot: BridgeOutboxSnapshot = PreviewData.outboxSnapshot()
        ) {
            library = PreviewData.storageLibrary(rows: rows)
            initialSelection = selectedReleaseId.map { [$0] } ?? []
            initialInspectorPresented = inspectorPresented
            initialInspectorTab = inspectorTab
            downloadStore = PreviewData.downloadStore(downloadSnapshot)
            outputStore = PreviewData.outputStore(outputSnapshot)
            outboxStore = PreviewData.outboxStore(outboxSnapshot)
        }

        var body: some View {
            StorageManagerView(
                initialSelection: initialSelection,
                initialInspectorPresented: initialInspectorPresented,
                initialInspectorTab: initialInspectorTab
            )
            .environment(library)
            .environment(
                StorageManagerStore(
                    library: library,
                    libraryStore: libraryStore,
                    onError: { uiStore.showError($0) }
                )
            )
            .environment(ImageStore.stub())
            .environment(libraryStore)
            .environment(ReleaseEditor.stub())
            .environment(Sync.stub())
            .environment(Downloads.stub())
            .environment(Outputs.stub())
            .environment(PreviewData.configStore())
            .environment(uiStore)
            .environment(downloadStore)
            .environment(outputStore)
            .environment(outboxStore)
        }
    }

    #Preview("Dense — wide, inspector open") {
        StorageManagerPreviewScene(
            selectedReleaseId: "rel-row-1",
            inspectorPresented: true
        )
        .frame(width: 1_440, height: 900)
    }

    #Preview("Dense — standard, inspector open") {
        StorageManagerPreviewScene(
            selectedReleaseId: "rel-row-1",
            inspectorPresented: true
        )
        .frame(width: 940, height: 760)
    }

    #Preview("Dense — compact") {
        StorageManagerPreviewScene()
            .frame(width: 700, height: 400)
    }

    #Preview("Dense — compact, selected") {
        StorageManagerPreviewScene(
            selectedReleaseId: "rel-row-4",
            downloadSnapshot: PreviewData.emptyDownloadSnapshot,
            outputSnapshot: PreviewData.emptyOutputSnapshot,
            outboxSnapshot: PreviewData.outboxSnapshot(
                uploadGroups: [],
                deletes: []
            )
        )
        .frame(width: 700, height: 400)
    }

    #Preview("Empty") {
        StorageManagerPreviewScene(rows: [])
            .frame(width: 940, height: 600)
    }

    #Preview("Empty-ish") {
        StorageManagerPreviewScene(
            rows: Array(PreviewData.storageRows.prefix(2))
        )
        .frame(width: 940, height: 600)
    }

    #Preview("Empty-ish — inspector open") {
        StorageManagerPreviewScene(
            rows: Array(PreviewData.storageRows.prefix(2)),
            inspectorPresented: true
        )
        .frame(width: 940, height: 600)
    }

    #Preview("One sync upload — inspector open") {
        StorageManagerPreviewScene(
            rows: Array(PreviewData.storageRows.prefix(2)),
            selectedReleaseId: "rel-row-2",
            inspectorPresented: true,
            inspectorTab: .transfers,
            downloadSnapshot: PreviewData.emptyDownloadSnapshot,
            outputSnapshot: PreviewData.emptyOutputSnapshot,
            outboxSnapshot: PreviewData.outboxSnapshot(
                uploadGroups: [PreviewData.uploadGroupDone],
                deletes: []
            )
        )
        .frame(width: 940, height: 600)
    }
#endif
