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
    @State
    private var selection: Set<String> = []
    @State
    private var inspectorPresented: Bool
    /// Runs row context-menu transitions, built once the environment is up.
    @State
    private var runner: StorageActionRunner?

    /// The smallest window the list and its inspector fit.
    fileprivate static let minimumWidth: CGFloat = 700
    fileprivate static let minimumHeight: CGFloat = 400

    /// The file whose details the inspector opens on, for the release it
    /// first shows.
    private let initialSelectedFileId: String?

    init(
        initialSelection: Set<String> = [],
        initialInspectorPresented: Bool = false,
        initialSelectedFileId: String? = nil
    ) {
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
        .toolbar {
            ToolbarItem(placement: .principal) {
                Picker("Filter", selection: $filter) {
                    Text("All").tag(BridgeStorageFilter.all)
                    Text("Local").tag(BridgeStorageFilter.local)
                    Text("Cloud").tag(BridgeStorageFilter.remote)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }
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
                    storageManagerStore: storageManagerStore,
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
        .onDisappear {
            storageManagerStore.cancel()
        }
    }

    private var releaseList: some View {
        VStack(spacing: 0) {
            if let list = storageManagerStore.list, let runner {
                if let error = list.initialLoadError {
                    LoadFailureView(error: error) {
                        Task { await list.loadInitial() }
                    }
                }
                else {
                    StorageTableView(
                        list: list,
                        selection: $selection,
                        sort: $sort,
                        inspectorPresented: $inspectorPresented,
                        runner: runner,
                    )
                    Divider()
                    StorageFooter(
                        list: list,
                        totalSize: storageManagerStore.totalSize,
                        queues: storageManagerStore.queues
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
    struct StorageManagerPreviewFixture {
        let library: Library
        let storageManagerStore: StorageManagerStore
        let uiStore: UiStore
        let downloadStore: DownloadStore
        let outputStore: OutputStore
        let outboxStore: OutboxStore

        let initialSelection: Set<String>
        let initialInspectorPresented: Bool
        let initialSelectedFileId: String?

        init(
            rows: [BridgeStorageRow] = PreviewData.storageRows,
            selectedReleaseId: String? = nil,
            inspectorPresented: Bool = false,
            selectedFileId: String? = nil,
            downloads: [BridgeDownloadOp] = PreviewData.downloadOps,
            outputs: [BridgeOutputOp] = PreviewData.outputOps,
            uploads: [PreviewData.PreviewReleaseUpload] =
                PreviewData.defaultUploads
        ) {
            let library = PreviewData.storageLibrary(
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

    // Keep every dependency on each #Preview root: the environment audit reads
    // the preview closure's modifier chain rather than following fixture views.
    #Preview("Dense — wide, inspector open") {
        let fixture = StorageManagerPreviewFixture(
            selectedReleaseId: "rel-row-1",
            inspectorPresented: true
        )
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(width: 1_440, height: 900)
    }

    #Preview("Dense — standard, inspector open") {
        let fixture = StorageManagerPreviewFixture(
            selectedReleaseId: "rel-row-1",
            inspectorPresented: true
        )
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(width: 940, height: 760)
    }

    #Preview("Dense — compact") {
        let fixture = StorageManagerPreviewFixture()
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(
            width: StorageManagerView.minimumWidth,
            height: StorageManagerView.minimumHeight
        )
    }

    #Preview("Dense — compact, selected") {
        let fixture = StorageManagerPreviewFixture(
            selectedReleaseId: "rel-row-4",
            downloads: [],
            outputs: [],
            uploads: []
        )
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(
            width: StorageManagerView.minimumWidth,
            height: StorageManagerView.minimumHeight
        )
    }

    #Preview("Empty") {
        let fixture = StorageManagerPreviewFixture(rows: [])
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(width: PreviewWindow.width, height: PreviewWindow.height)
    }

    #Preview("Empty-ish") {
        let fixture = StorageManagerPreviewFixture(
            rows: Array(PreviewData.storageRows.prefix(2))
        )
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(width: PreviewWindow.width, height: PreviewWindow.height)
    }

    #Preview("Empty-ish — inspector open") {
        let fixture = StorageManagerPreviewFixture(
            rows: Array(PreviewData.storageRows.prefix(2)),
            inspectorPresented: true
        )
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(width: PreviewWindow.width, height: PreviewWindow.height)
    }

    #Preview("One sync upload — inspector open") {
        let fixture = StorageManagerPreviewFixture(
            rows: Array(PreviewData.storageRows.prefix(2)),
            selectedReleaseId: "rel-row-2",
            inspectorPresented: true,
            downloads: [],
            outputs: [],
            uploads: [PreviewData.queuedRelease]
        )
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(width: PreviewWindow.width, height: PreviewWindow.height)
    }

    /// An upload waiting because its source file is missing, with the reason
    /// in the inspector.
    #Preview("A waiting upload — inspector open") {
        let fixture = StorageManagerPreviewFixture(
            rows: Array(PreviewData.storageRows.prefix(3)),
            selectedReleaseId: "rel-row-3",
            inspectorPresented: true,
            downloads: [],
            outputs: [],
            uploads: [PreviewData.pendingRelease]
        )
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(width: PreviewWindow.width, height: PreviewWindow.height)
    }
    /// A release pinning, the table's other rows uploading and pinning too.
    #Preview("Pinning — inspector open") {
        let fixture = StorageManagerPreviewFixture(
            selectedReleaseId: "rel-row-5",
            inspectorPresented: true,
            downloads: PreviewData.pinningDownloads,
            outputs: []
        )
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(width: PreviewWindow.width, height: PreviewWindow.height)
    }

    /// A release whose pin waits its turn in the download queue.
    #Preview("Pin queued — inspector open") {
        let fixture = StorageManagerPreviewFixture(
            selectedReleaseId: "rel-row-5",
            inspectorPresented: true,
            downloads: PreviewData.queuedPinDownloads,
            outputs: [],
            uploads: []
        )
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(width: PreviewWindow.width, height: PreviewWindow.height)
    }

    /// A file of a release uploading selected: its details under the list.
    #Preview("A file's details — inspector open") {
        let fixture = StorageManagerPreviewFixture(
            selectedReleaseId: "rel-row-1",
            inspectorPresented: true,
            selectedFileId: "rel-row-1-audio",
            downloads: [],
            outputs: []
        )
        StorageManagerView(
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: fixture.initialInspectorPresented,
            initialSelectedFileId: fixture.initialSelectedFileId
        )
        .environment(fixture.library)
        .environment(fixture.storageManagerStore)
        .environment(ImageStore.stub())
        .environment(ReleaseEditor.stub())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(fixture.uiStore)
        .environment(fixture.downloadStore)
        .environment(fixture.outputStore)
        .environment(fixture.outboxStore)
        .frame(width: PreviewWindow.width, height: PreviewWindow.height)
    }
#endif
