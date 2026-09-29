import BaeKit
import Testing

@testable import bae

/// The filter field shows core's count only beside the filter core counted
/// under, so a count never outlives the text or states it was for.
@Suite("The filter field's count")
struct NarrowedCountTests {
    private func summary(text: String) -> BridgeImportQueueSummary {
        PreviewData.importQueueSummary(
            pending: 84,
            done: 0,
            skipped: 0,
            watchedFolders: [],
            narrowed: BridgeNarrowedCount(shown: 7, total: 84),
            filterText: text
        )
    }

    private func narrowing(
        text: String,
        states: [BridgePendingState] = []
    ) -> BridgeImportListNarrowing {
        BridgeImportListNarrowing(
            tab: .pending,
            filterText: text,
            pendingFilters: states
        )
    }

    @Test("a count stands beside the filter it was counted under")
    func aCountForTheFilterShown() {
        #expect(
            summary(text: "melv").narrowedCount(under: narrowing(text: "melv"))
                == BridgeNarrowedCount(shown: 7, total: 84)
        )
    }

    @Test("a count for other text, or cleared text, is not shown")
    func aCountForOtherTextIsNot() {
        #expect(
            summary(text: "melv").narrowedCount(under: narrowing(text: ""))
                == nil
        )
        #expect(
            summary(text: "mel").narrowedCount(under: narrowing(text: "melv"))
                == nil
        )
    }

    @Test("a count for other states is not shown")
    func aCountForOtherStatesIsNot() {
        #expect(
            summary(text: "melv")
                .narrowedCount(
                    under: narrowing(text: "melv", states: [.needsYou])
                ) == nil
        )
    }
}
