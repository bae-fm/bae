import BaeKit
import SwiftUI

/// The buttons at the end of the import list's filter row: the indicators of
/// the work that runs, then the menu of sorting, filtering and watched
/// folders once bae knows whether it watches any. `onChange` runs before a
/// change of the list the person makes.
struct ImportListControls: View {
    let importStore: ImportStore
    let listSlot: ImportListSlot
    let onGoToUnidentified: () -> Void
    /// Take every candidate off the identification queue.
    let onCancelAllIdentification: () -> Void
    /// Cancel every import that has not begun writing its release.
    let onCancelAllImports: () -> Void
    let onAddFolder: () -> Void
    let onRefreshFolder: (_ folder: BridgeWatchedFolderScanStatus) -> Void
    let onRemoveFolder: (_ path: Data) -> Void
    let onChange: () -> Void

    @Environment(UiStore.self)
    private var uiStore

    private var summary: BridgeImportQueueSummary? {
        importStore.summary
    }

    var body: some View {
        IdentificationProgressSlot(
            importStore: importStore,
            onGoToUnidentified: onGoToUnidentified,
            onCancelAll: onCancelAllIdentification
        )
        ImportActivitySlot(
            importStore: importStore,
            onCancelAll: onCancelAllImports
        )
        FolderScanActivitySlot(importStore: importStore)
        if importStore.hasWatchedFolders != nil {
            CandidateListMenu(
                refreshingFolders: uiStore.refreshingWatchedFolders,
                hasGroups: summary?.hasGroups ?? false,
                sortOrder: listSlot.sortOrder,
                pendingFilter: uiStore
                    .importCandidatePendingFilter,
                importStore: importStore,
                pendingFilterApplies: uiStore
                    .importCandidateTab == .pending,
                actions: CandidateListMenuActions(
                    onSetSortOrder: { order in
                        onChange()
                        listSlot.setSortOrder(order)
                    },
                    onSetPendingFilter: { filter in
                        onChange()
                        listSlot.setPendingFilter(filter)
                    },
                    onAddFolder: onAddFolder,
                    onSetAllGroupsExpanded: { expanded in
                        onChange()
                        listSlot.setAllGroupsExpanded(expanded)
                    },
                    onRefreshFolder: onRefreshFolder,
                    onRemoveFolder: onRemoveFolder
                )
            )
        }
    }
}
