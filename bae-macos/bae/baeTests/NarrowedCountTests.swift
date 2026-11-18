import BaeKit
import Testing

@testable import bae

/// The filter field shows core's count only beside the filter that the slot
/// opened its list under, so the field never shows a count for other text or
/// another entry.
@MainActor
@Suite("The filter field's count")
struct NarrowedCountTests {
    /// A slot that read its list under the filter text `text`, with a count
    /// of 7 of 84.
    private func slot(text: String) async throws -> ImportListSlot {
        let uiStore = UiStore()
        uiStore.setImportCandidateFilterText(text)
        let handle = ImportListHandle.stub(
            queries: { _ in
                .fixed(
                    [],
                    answers: UnansweredCount(),
                    extra: BridgeListCountExtra.import(
                        narrowed: BridgeNarrowedCount(shown: 7, total: 84),
                        tabs: BridgeTriageTabCounts(
                            pending: 84,
                            done: 0,
                            skipped: 0
                        )
                    )
                )
            }
        )
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: { handle }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.count != nil }
        return slot
    }

    private func narrowing(
        text: String,
        filter: BridgePendingFilter = .all
    ) -> ImportListNarrowing {
        ImportListNarrowing(
            tab: .pending,
            filterText: text,
            pendingFilter: filter
        )
    }

    @Test("a count stands beside the filter it was counted under")
    func aCountForTheFilterShown() async throws {
        let slot = try await slot(text: "melv")
        #expect(
            slot.narrowedCount(under: narrowing(text: "melv"))
                == BridgeNarrowedCount(shown: 7, total: 84)
        )
    }

    @Test("a count for other text, or cleared text, is not shown")
    func aCountForOtherTextIsNot() async throws {
        let slot = try await slot(text: "melv")
        #expect(slot.narrowedCount(under: narrowing(text: "")) == nil)
        #expect(slot.narrowedCount(under: narrowing(text: "mel")) == nil)
    }

    @Test("a count for another filter entry is not shown")
    func aCountForAnotherEntryIsNot() async throws {
        let slot = try await slot(text: "melv")
        #expect(
            slot.narrowedCount(
                under: narrowing(text: "melv", filter: .needsYou)
            ) == nil
        )
    }
}
