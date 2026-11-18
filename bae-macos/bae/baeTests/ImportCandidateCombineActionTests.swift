import BaeKit
import Foundation
import Testing

@testable import bae

private struct CombineRefused: Error {}

@MainActor
@Suite("Combining the selected folders")
struct ImportCandidateCombineActionTests {
    @Test("the selection is combined in core, and the release is revealed")
    func combinesTheSelectionAndRevealsTheRelease() async throws {
        let uiStore = UiStore()
        let combined = CallLog<Void>()

        await ImportCandidateCombineAction(
            selection: ImportSelection(
                operations: .stub(combine: { _ in
                    combined.record(())
                    return Data("grouping-new".utf8)
                })
            ),
            uiStore: uiStore
        )
        .run()

        #expect(combined.all.count == 1)
        let reveals = RequestListener(uiStore.importCandidateReveals)
        defer { reveals.stop() }
        try await Wait.until { !reveals.received.isEmpty }
        #expect(reveals.received == [Data("grouping-new".utf8)])
        #expect(uiStore.lastError == nil)
    }

    @Test("a refused combine reveals nothing and reports why")
    func failureRevealsNothing() async throws {
        let uiStore = UiStore()

        await ImportCandidateCombineAction(
            selection: ImportSelection(
                operations: .stub(combine: { _ in throw CombineRefused() })
            ),
            uiStore: uiStore
        )
        .run()

        // Nothing waits for the list: the first request a listener gets is
        // one sent after it began to listen.
        let reveals = RequestListener(uiStore.importCandidateReveals)
        defer { reveals.stop() }
        uiStore.navigateToImportCandidate(Data("later".utf8))
        try await Wait.until { !reveals.received.isEmpty }
        #expect(reveals.received == [Data("later".utf8)])
        #expect(uiStore.lastError != nil)
    }
}
