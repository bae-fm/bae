import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportFilterRow`: resting, with text typed, while a batch
    /// identifies, while imports run, while folders scan, with a filter on,
    /// and with all of it at once.
    static let macImportFilterRowScenes: [ShotScene] = [
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

    private static func filterRowScene(
        _ id: String,
        _ state: ColumnState
    ) -> ShotScene {
        component(id, width: 420, height: 52) {
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
}
