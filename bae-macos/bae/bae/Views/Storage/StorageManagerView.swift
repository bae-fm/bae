import BaeKit
import SwiftUI

struct StorageManagerView: View {
    @Environment(StorageManagerStore.self)
    private var storageManagerStore
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
    // This window is its own scene, so it shows its own errors.
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var filter: BridgeStorageFilter = .all
    @State
    private var sort = BridgeStorageSort(
        field: .albumTitle,
        direction: .ascending
    )
    /// What the filter field holds, which narrows the list to the releases
    /// whose album library search finds.
    @State
    private var search: String
    @State
    private var selection: Set<String> = []
    @State
    private var inspectorPresented: Bool
    /// The table's columns as the person last arranged them.
    @AppStorage("storageManager.columnLayout")
    private var columnLayout = StorageTableLayout.standard
    /// Runs row context-menu transitions, built once the environment is up.
    @Environment(SettingsNavigation.self)
    private var settingsNavigation
    @Environment(\.openSettingsWindow)
    private var openSettings
    @State
    private var runner: StorageActionRunner?

    /// The smallest window the list and its inspector fit.
    fileprivate static let minimumWidth: CGFloat = 700
    fileprivate static let minimumHeight: CGFloat = 400

    /// The file whose details the inspector opens on, for the release it
    /// first shows.
    private let initialSelectedFileId: String?

    init(
        initialSearch: String = "",
        initialSelection: Set<String> = [],
        initialInspectorPresented: Bool = false,
        initialSelectedFileId: String? = nil
    ) {
        _search = State(initialValue: initialSearch)
        _selection = State(initialValue: initialSelection)
        _inspectorPresented = State(
            initialValue: initialInspectorPresented
        )
        self.initialSelectedFileId = initialSelectedFileId
    }

    var body: some View {
        HSplitView {
            releaseList
                .frame(minWidth: 440, maxHeight: .infinity)

            if inspectorPresented {
                // Only the part of the selection the list holds, so the
                // inspector never shows a release the list doesn't.
                StorageInspector(
                    releaseId: StorageInspector.releaseId(
                        in: storageManagerStore.shown(selection)
                    ),
                    initialSelectedFileId: initialSelectedFileId
                )
            }
        }
        .frame(
            minWidth: Self.minimumWidth,
            minHeight: Self.minimumHeight
        )
        .searchable(
            text: $search,
            placement: .toolbar,
            prompt: Text("Filter")
        )
        .toolbar {
            ToolbarItem(placement: .principal) {
                Picker("Filter", selection: $filter) {
                    Text("All").tag(BridgeStorageFilter.all)
                    Text("Local").tag(BridgeStorageFilter.local)
                    Text("Cloud").tag(BridgeStorageFilter.remote)
                }
                .pickerStyle(.segmented)
                .accentFocusRing(cornerRadius: .infinity)
                .labelsHidden()
            }
            ToolbarItem(placement: .primaryAction) {
                Toggle(isOn: $inspectorPresented) {
                    Label("Inspector", systemImage: "sidebar.trailing")
                        .labelStyle(.iconOnly)
                }
                .toggleStyle(.button)
                .foregroundStyle(
                    inspectorPresented ? Theme.accent : Color.secondary
                )
                .accentFocusRing(cornerRadius: .infinity)
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
                    storageManagerStore: storageManagerStore,
                    openSaveAsSettings: {
                        settingsNavigation.open(
                            .formats,
                            present: { openSettings() }
                        )
                    }
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
                    onConfirm: { runner.confirmMoveToCloud() },
                    onCancel: { runner.cancelMoveToCloud() },
                )
                .frame(width: 420)
            }
        }
        .unpinConfirmation(
            pending: Binding(
                get: { runner?.pendingUnpin },
                set: { runner?.pendingUnpin = $0 }
            )
        ) { pending, answer in
            runner?.answerUnpin(pending, answer)
        }
        .errorAlert(uiStore)
        .task {
            storageManagerStore.select(selection)
            updateQuery()
        }
        .onChange(of: selection) { _, selection in
            storageManagerStore.select(selection)
        }
        .onChange(of: filter) { _, _ in
            // Selection is scoped to the visible tab.
            selection = []
            updateQuery()
        }
        .onChange(of: sort) { _, _ in updateQuery() }
        .onChange(of: search) { _, _ in updateQuery() }
        .onDisappear {
            storageManagerStore.cancel()
        }
    }

    private var releaseList: some View {
        VStack(spacing: 0) {
            if let list = storageManagerStore.list, let runner {
                if let error = list.initialLoadError {
                    Notice(
                        error: error,
                        look: .centered,
                        action: error.retryAction {
                            Task { await list.loadInitial() }
                        }
                    )
                }
                else {
                    StorageTableView(
                        list: list,
                        selection: $selection,
                        sort: $sort,
                        inspectorPresented: $inspectorPresented,
                        layout: $columnLayout,
                        runner: runner,
                    )
                    .overlay {
                        if storageManagerStore.searching, list.totalCount == 0 {
                            ContentUnavailableView {
                                Text("No matches")
                            }
                        }
                    }
                    Divider()
                    StorageFooter(
                        list: list,
                        totalSize: storageManagerStore.totalSize,
                        queues: storageManagerStore.queues
                    )
                }
            }
            else {
                // The first page is a local read; nothing shows until it lands.
                Color.clear
            }
        }
    }

    private func updateQuery() {
        storageManagerStore.update(filter: filter, sort: sort, search: search)
    }
}

#if DEBUG
    @MainActor
    struct StorageManagerPreviewFixture {
        let library: Library
        let storageManagerStore: StorageManagerStore
        let uiStore: UiStore
        let downloadStore: DownloadStore
        let outputStore: OutputStore
        let outboxStore: OutboxStore

        let initialSearch: String
        let initialSelection: Set<String>
        let initialInspectorPresented: Bool
        let initialSelectedFileId: String?

        init(
            rows: [BridgeStorageRow] = PreviewData.storageRows,
            search: String = "",
            selectedReleaseId: String? = nil,
            inspectorPresented: Bool = false,
            selectedFileId: String? = nil,
            downloads: [BridgeDownloadOp] = PreviewData.downloadOps,
            outputs: [BridgeOutputOp] = PreviewData.outputOps,
            uploads: [PreviewData.PreviewReleaseUpload] =
                PreviewData.defaultUploads,
            answers: UnansweredCount = UnansweredCount()
        ) {
            let library = PreviewData.storageLibrary(
                answers: answers,
                rows: rows,
                downloads: downloads,
                outputs: outputs,
                uploads: uploads
            )
            let uiStore = UiStore()
            self.library = library
            self.storageManagerStore = StorageManagerStore(
                library: library,
                onError: { uiStore.showError($0) }
            )
            self.uiStore = uiStore
            initialSearch = search
            initialSelection = selectedReleaseId.map { [$0] } ?? []
            initialInspectorPresented = inspectorPresented
            initialSelectedFileId = selectedFileId
            downloadStore = PreviewData.downloadStore(downloads: downloads)
            outputStore = PreviewData.outputStore()
            outboxStore = PreviewData.outboxStore()
        }
    }

    /// The window most previews draw in.
    private enum PreviewWindow {
        static let width: CGFloat = 940
        static let height: CGFloat = 600
    }

#endif
