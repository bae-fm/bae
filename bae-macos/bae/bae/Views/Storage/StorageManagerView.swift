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
    // The queue sections and inspector read their stores at the leaf; uiStore
    // is read here to surface queue action errors in this window (it's a
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
    /// Runs row context-menu transitions; built lazily once the services are
    /// available from the environment.
    @State
    private var runner: StorageActionRunner?

    var body: some View {
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
                    HSplitView {
                        VStack(spacing: 0) {
                            StorageTableView(
                                list: list,
                                selection: $selection,
                                sort: $sort,
                                sortingEnabled: filter != .uploading,
                                libraryStore: libraryStore,
                                library: library,
                                runner: runner,
                            )

                            DownloadsSection()
                            OutputSection()
                            OutboxSection()
                            Divider()
                            StorageFooter(
                                list: list,
                                totalSize: storageManagerStore.totalSize
                            )
                        }
                        .frame(minWidth: 480)

                        if let releaseId = StorageTransferInspector.releaseId(
                            in: selection
                        ) {
                            StorageTransferInspector(
                                releaseId: releaseId,
                                selection: $selection
                            )
                        }
                    }
                }
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
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

    private func updateQuery() {
        storageManagerStore.update(filter: filter, sort: sort)
    }
}

#if DEBUG
    #Preview("Full screen") {
        let library = PreviewData.storageLibrary()
        let libraryStore = LibraryStore()
        let uiStore = UiStore()
        StorageManagerView()
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
            .environment(PreviewData.downloadStore())
            .environment(PreviewData.outputStore())
            .environment(PreviewData.outboxStore())
            .frame(width: 940, height: 760)
    }
#endif
