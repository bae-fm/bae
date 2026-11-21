import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macImportTabsScenes: [ShotScene] = [
        importTabsScene("mac.component.import-tabs.found-open"),
        importTabsScene("mac.component.import-tabs.imported-open", tab: .done),
        importTabsScene(
            "mac.component.import-tabs.skipped-open",
            tab: .skipped
        ),
        importTabsScene(
            "mac.component.import-tabs.nothing-found",
            counts: BridgeTriageTabCounts(pending: 0, done: 31, skipped: 0)
        ),
    ]

    private static func listSlot(
        _ importStore: ImportStore,
        _ uiStore: UiStore,
        counts: BridgeTriageTabCounts
    ) -> ImportListSlot {
        ImportListSlot.preview(
            importStore: importStore,
            uiStore: uiStore,
            items: [],
            tabCounts: counts
        )
    }

    private static let listCounts = BridgeTriageTabCounts(
        pending: 14,
        done: 31,
        skipped: 2
    )

    private static func importTabsScene(
        _ id: String,
        tab: BridgeTriageTab = .pending,
        counts: BridgeTriageTabCounts = listCounts
    ) -> ShotScene {
        component(
            id,
            width: 420,
            height: 60
        ) {
            importStores(
                MacImportTabs(
                    activeTab: .constant(tab),
                    listSlot: listSlot(ImportStore(), UiStore(), counts: counts)
                )
                .frame(width: 420, alignment: .leading)
            )
        }
    }
}
