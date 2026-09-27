import BaeKit
import Foundation
import Testing

@testable import bae

private struct CombineRefused: Error {}

@MainActor
@Suite("Combining the selected folders")
struct ImportCandidateCombineActionTests {
    @Test("the selection is combined in core, and the release is revealed")
    func combinesTheSelectionAndRevealsTheRelease() async {
        let uiStore = UiStore()
        let combined = CallLog<Void>()

        await ImportCandidateCombineAction(
            selection: ImportSelection(
                operations: .stub(combine: {
                    combined.record(())
                    return "grouping-new"
                })
            ),
            uiStore: uiStore
        )
        .run()

        #expect(combined.all.count == 1)
        #expect(
            uiStore.pendingImportCandidateReveal?.candidateKey == "grouping-new"
        )
        #expect(uiStore.lastError == nil)
    }

    @Test("a refused combine reveals nothing and reports why")
    func failureRevealsNothing() async {
        let uiStore = UiStore()

        await ImportCandidateCombineAction(
            selection: ImportSelection(
                operations: .stub(combine: { throw CombineRefused() })
            ),
            uiStore: uiStore
        )
        .run()

        #expect(uiStore.pendingImportCandidateReveal == nil)
        #expect(uiStore.lastError != nil)
    }
}
