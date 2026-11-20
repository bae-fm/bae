import BaeKit
import SwiftUI

@testable import bae

// The import list's column: its filter row and sidebar, drawn through the
// sidebar's container, and the window around it.

extension ShotScene {
    static let importSidebarComponentScenes: [ShotScene] =
        filterRowScenes + sidebarScenes + importWindowScenes

    /// What the list's column shows beyond its two rows: the text typed and
    /// the count it narrows to, the work that runs, and the filter that is
    /// on.
    private struct ColumnState {
        var tab: BridgeTriageTab = .pending
        var typed = ""
        var narrowed: BridgeNarrowedCount?
        var identifying = false
        var importing = false
        var scanning = false
        var filtered = false
        var items: [BridgeImportListItem] = PreviewData.lookupItems()
    }

    private static let columnCounts = BridgeTriageTabCounts(
        pending: 14,
        done: 31,
        skipped: 2
    )

    /// The store the column reads: a watched folder, and the work `state`
    /// says runs.
    @MainActor
    private static func columnStore(_ state: ColumnState) -> ImportStore {
        let store = ImportStore()
        store.applySummary(
            PreviewData.importQueueSummary(
                hasWatchedFolders: true,
                hasGroups: false
            )
        )
        if state.identifying {
            store.identificationProgress = (identified: 4, total: 10)
        }
        if state.importing {
            store.importsInFlight = 2
        }
        if state.scanning {
            store.applyFolderScans(
                BridgeFolderScans(
                    statuses: [],
                    hasProblem: false,
                    scanning: true,
                    alerts: []
                )
            )
        }
        return store
    }

    /// The column as the app builds it: the container over a store and a
    /// slot that hold `state`.
    @MainActor
    private static func column(_ state: ColumnState) -> some View {
        let store = columnStore(state)
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(state.tab)
        uiStore.setImportCandidateFilterText(state.typed)
        uiStore.setImportCandidatePendingFilter(
            state.filtered ? .needsYou : .all
        )
        let fixture = ImportPreviewFixture(
            store: store,
            itemsByTab: [state.tab: state.items],
            tabCounts: columnCounts
        )
        return MacImportSidebarContainer(
            importStore: store,
            listSlot: fixture.slot(
                uiStore: uiStore,
                narrowed: state.narrowed
            ),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onCombineFolder: { _ in },
            onSeparate: { _ in },
            onShowInLibrary: { _ in },
            onPerform: { _ in },
            onCancelAllIdentification: {},
            onCancelAllImports: {}
        )
        .environment(uiStore)
        .environment(store)
    }

    // MARK: Filter row

    private static func filterRowScene(
        _ id: String,
        _ state: ColumnState
    ) -> ShotScene {
        component(id, width: 436, height: 52) {
            importStores(
                MacImportFilterRow(
                    filterText: .constant(state.typed),
                    narrowed: { state.narrowed },
                    trailing: { filterControls(state) }
                )
                .frame(width: 420)
            )
        }
    }

    /// The buttons at the end of the filter row, the controls the sidebar's
    /// container puts there.
    @MainActor
    private static func filterControls(_ state: ColumnState) -> some View {
        let store = columnStore(state)
        let uiStore = UiStore()
        uiStore.setImportCandidatePendingFilter(
            state.filtered ? .needsYou : .all
        )
        return ImportListControls(
            importStore: store,
            listSlot: ImportPreviewFixture(
                store: store,
                itemsByTab: [:],
                tabCounts: columnCounts
            )
            .slot(uiStore: uiStore),
            onGoToUnidentified: {},
            onCancelAllIdentification: {},
            onCancelAllImports: {},
            onAddFolder: {},
            onRefreshFolder: { _ in },
            onRemoveFolder: { _ in },
            onChange: {}
        )
        .environment(uiStore)
        .environment(store)
    }

    private static let narrowedToTwo = BridgeNarrowedCount(shown: 2, total: 14)

    /// `MacImportFilterRow`: resting, with text typed, while a batch
    /// identifies, while imports run, while folders scan, with a filter on,
    /// and with all of it at once.
    private static let filterRowScenes: [ShotScene] = [
        filterRowScene(
            "mac.component.import-filter-row.resting",
            ColumnState()
        ),
        filterRowScene(
            "mac.component.import-filter-row.typed",
            ColumnState(typed: "tide", narrowed: narrowedToTwo, filtered: true)
        ),
        filterRowScene(
            "mac.component.import-filter-row.identifying",
            ColumnState(identifying: true)
        ),
        filterRowScene(
            "mac.component.import-filter-row.importing",
            ColumnState(importing: true)
        ),
        filterRowScene(
            "mac.component.import-filter-row.scanning",
            ColumnState(scanning: true)
        ),
        filterRowScene(
            "mac.component.import-filter-row.filtered",
            ColumnState(filtered: true)
        ),
        filterRowScene(
            "mac.component.import-filter-row.all-at-once",
            ColumnState(
                typed: "tide",
                narrowed: narrowedToTwo,
                identifying: true,
                importing: true,
                scanning: true,
                filtered: true
            )
        ),
    ]

    // MARK: Sidebar

    private static func sidebarScene(
        _ id: String,
        _ state: ColumnState
    ) -> ShotScene {
        component(
            id,
            width: 420,
            height: 360,
            inset: 0,
            images: PreviewData.artImageStore()
        ) {
            importStores(
                column(state)
                    .frame(width: 420, height: 360, alignment: .top)
                    .background(Theme.surface)
            )
        }
    }

    /// `MacImportSidebar` on each tab, with text typed, while work runs and
    /// with no match.
    private static let sidebarScenes: [ShotScene] = [
        sidebarScene("mac.component.import-sidebar.found", ColumnState()),
        sidebarScene(
            "mac.component.import-sidebar.imported-open",
            ColumnState(tab: .done)
        ),
        sidebarScene(
            "mac.component.import-sidebar.skipped-open",
            ColumnState(tab: .skipped)
        ),
        sidebarScene(
            "mac.component.import-sidebar.filtered",
            ColumnState(
                typed: "tide",
                narrowed: BridgeNarrowedCount(shown: 1, total: 14),
                filtered: true,
                items: [PreviewData.lookupItems()[0]]
            )
        ),
        sidebarScene(
            "mac.component.import-sidebar.busy",
            ColumnState(identifying: true, importing: true, scanning: true)
        ),
        sidebarScene(
            "mac.component.import-sidebar.empty",
            ColumnState(
                typed: "harbor",
                narrowed: BridgeNarrowedCount(shown: 0, total: 14),
                filtered: true,
                items: []
            )
        ),
    ]

    // MARK: Window

    /// The Import window's content as `board` draws it, with no work running
    /// and the list holding two rows, at the preview's 1000 by 420 points.
    @MainActor
    private static func importWindow(
        _ board: PreviewData.ImportBoard
    ) -> some View {
        let uiStore = UiStore()
        uiStore.activeSection = .importing
        uiStore.setImportCandidateTab(board.tab)
        let scene = PreviewData.importBoardScene(board)
        scene.fixture.store.identificationProgress = nil
        scene.fixture.store.importsInFlight = 0
        let fixture = ImportPreviewFixture(
            store: scene.fixture.store,
            itemsByTab: [board.tab: PreviewData.lookupItems()],
            tabCounts: columnCounts
        )
        let slot = fixture.slot(uiStore: uiStore, selection: scene.selection)
        return MacImportWindow(endEditing: {})
            .environment(uiStore)
            .importSectionEnvironment(
                store: fixture.store,
                slot: slot,
                runtimes: scene.runtimes
            )
            .environment(SettingsNavigation())
            .environment(PreviewAudio.stub())
            .environment(Playback.stub())
            .environment(Library.stub())
            .environment(Importer.stub())
            .environment(PreviewData.releaseEditor())
            .environment(
                PreviewData.configStore(storage: PreviewData.previewStorage)
            )
            .environment(scene.outbox)
            .environment(\.importPercents, scene.importPercents)
            .mainWindowPreviewEnvironment()
            .frame(width: 1_000, height: 420)
    }

    /// `MacImportWindow` with a folder picked and with none.
    private static let importWindowScenes: [ShotScene] = [
        component(
            "mac.component.import-window.folder-picked",
            width: 1_000,
            height: 420,
            inset: 0,
            images: PreviewData.artImageStore()
        ) {
            importWindow(.found(northbound: .identified, opening: .copperSky))
        },
        component(
            "mac.component.import-window.no-selection",
            width: 1_000,
            height: 420,
            inset: 0,
            images: PreviewData.artImageStore()
        ) {
            importWindow(.found(northbound: .identified, opening: nil))
        },
    ]
}
