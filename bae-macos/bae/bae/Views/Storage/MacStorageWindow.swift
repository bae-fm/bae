import BaeKit
import SwiftUI

/// The Storage Manager window's least size, which its list and inspector
/// fit, and which the window keeps while no library is open.
enum StorageManagerWindowSize {
    static let minimumWidth: CGFloat = 700
    static let minimumHeight: CGFloat = 400
}

/// The Storage Manager window: the release list, and beside it, while the
/// inspector is shown, the `Inspector` it hosts.
struct MacStorageWindow<Inspector: View>: View {
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
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var filter: BridgeStorageFilter
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

    /// What the window shows beside the list while the inspector is shown.
    private let inspector: () -> Inspector

    init(
        initialFilter: BridgeStorageFilter = .all,
        initialSearch: String = "",
        initialSelection: Set<String> = [],
        initialInspectorPresented: Bool = false,
        @ViewBuilder inspector: @escaping () -> Inspector
    ) {
        _filter = State(initialValue: initialFilter)
        _search = State(initialValue: initialSearch)
        _selection = State(initialValue: initialSelection)
        _inspectorPresented = State(
            initialValue: initialInspectorPresented
        )
        self.inspector = inspector
    }

    var body: some View {
        HSplitView {
            releaseList
                .frame(minWidth: 440, maxHeight: .infinity)

            if inspectorPresented {
                inspector()
            }
        }
        .frame(
            minWidth: StorageManagerWindowSize.minimumWidth,
            minHeight: StorageManagerWindowSize.minimumHeight
        )
        .windowBackground()
        .modifier(
            MacStorageToolbar(
                filter: $filter,
                search: $search,
                inspectorPresented: $inspectorPresented
            )
        )
        .onAppear {
            if runner == nil {
                runner = StorageActionRunner(
                    releaseEditor: releaseEditor,
                    sync: sync,
                    downloads: downloads,
                    outputs: outputs,
                    configStore: configStore,
                    uiStore: uiStore,
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
                    MacNotice(
                        error: error,
                        look: .centered,
                        action: error.retryAction {
                            Task { await list.loadInitial() }
                        }
                    )
                }
                else {
                    MacTable(
                        list: list,
                        selection: $selection,
                        sort: $sort,
                        inspectorPresented: $inspectorPresented,
                        layout: $columnLayout,
                        runner: runner,
                    )
                    // The table's rows and header sit this far in from
                    // the pane's edges, as the board draws them.
                    .padding(.horizontal, ThemeSpace.related + ThemeSpace.line)
                    .overlay {
                        if storageManagerStore.searching == true,
                            list.count?.rows == 0
                        {
                            // VoiceOver reads the table's column headings,
                            // then the words over the empty list, then the
                            // footer, as the board lists them.
                            MacContentUnavailable(title: "No matches")
                                .accessibilitySortPriority(-1)
                        }
                    }
                    Divider()
                    MacStorageFooter(
                        list: list,
                        totalSize: storageManagerStore.totalSize,
                        queues: storageManagerStore.queues
                    )
                    .accessibilitySortPriority(-2)
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

extension MacStorageWindow where Inspector == StorageInspector {
    /// The window with the storage inspector, which opens on the details of
    /// `initialSelectedFileId` for the release it first shows.
    init(
        initialFilter: BridgeStorageFilter = .all,
        initialSearch: String = "",
        initialSelection: Set<String> = [],
        initialInspectorPresented: Bool = false,
        initialSelectedFileId: String? = nil
    ) {
        self.init(
            initialFilter: initialFilter,
            initialSearch: initialSearch,
            initialSelection: initialSelection,
            initialInspectorPresented: initialInspectorPresented
        ) {
            StorageInspector(initialSelectedFileId: initialSelectedFileId)
        }
    }
}
