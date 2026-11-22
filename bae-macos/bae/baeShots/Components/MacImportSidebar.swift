import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportSidebar` on each tab, with text typed, while work runs and
    /// with no match.
    static let macImportSidebarScenes: [ShotScene] = [
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
                items: { [try PreviewData.lookupItems()[0]] }
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
                items: { [] }
            )
        ),
        sidebarScene(
            "mac.component.import-sidebar.narrow",
            ColumnState(),
            width: 300
        ),
    ]

    /// The column as the app builds it: the container over a store and a
    /// slot that hold `state`.
    @MainActor
    private static func column(_ state: ColumnState) throws -> some View {
        let store = columnStore(state)
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(state.tab)
        uiStore.setImportCandidateFilterText(state.typed)
        uiStore.setImportCandidatePendingFilter(
            state.filtered ? .needsYou : .all
        )
        let fixture = ImportPreviewFixture(
            store: store,
            itemsByTab: [state.tab: try state.items()],
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

    private static func sidebarScene(
        _ id: String,
        _ state: ColumnState,
        width: CGFloat = 420
    ) -> ShotScene {
        component(
            id,
            width: width,
            height: 360,
            images: PreviewData.artImageStore()
        ) {
            importStores(
                try column(state)
                    .frame(width: width, height: 360, alignment: .top)
                    .background(Theme.surface)
            )
        }
    }
}
