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
        selection: ImportSelection = ImportSelection()
    ) -> ImportListSlot {
        seededWithItems(
            importStore: importStore,
            uiStore: uiStore,
            items: items,
            selection: selection,
            pendingFilterEntries: PreviewData.pendingFilterEntries()
        )
    }
}
