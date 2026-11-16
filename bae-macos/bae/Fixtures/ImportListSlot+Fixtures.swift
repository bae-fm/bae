import BaeKit
import Foundation
import SwiftUI

@testable import bae

extension ImportListSlot {
    /// A slot over a fixed set of items, for shots and tests, with the
    /// fixtures' pending filter entries.
    static func preview(
        importStore: ImportStore,
        uiStore: UiStore,
        items: [BridgeImportListItem],
        tabCounts: BridgeTriageTabCounts = BridgeTriageTabCounts(
            pending: 0,
            done: 0,
            skipped: 0
        ),
        selection: ImportSelection = ImportSelection()
    ) -> ImportListSlot {
        importStore.applyPendingFilterEntries(
            PreviewData.pendingFilterEntries()
        )
        return seededWithItems(
            importStore: importStore,
            uiStore: uiStore,
            items: items,
            tabCounts: tabCounts,
            selection: selection
        )
    }
}
