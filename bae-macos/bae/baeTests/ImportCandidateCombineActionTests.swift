import BaeKit
import Combine
import Foundation
import Testing

@testable import bae

private struct CombineRefused: Error {}

private final class Recorder<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [Value] = []

    var all: [Value] {
        lock.withLock { values }
    }

    func record(_ value: Value) {
        lock.withLock { values.append(value) }
    }
}

@MainActor
@Suite("Combining the selected folders")
struct ImportCandidateCombineActionTests {
    @Test("the selected folders are combined, and the release is revealed")
    func combinesTheSelectionAndRevealsTheRelease() async {
        let uiStore = UiStore()
        uiStore.setFolderCandidateSelection([
            "/music/Volume B", "/music/Volume A",
        ])
        let listSlot = ImportListSlot.preview(
            importStore: ImportStore(),
            uiStore: uiStore,
            items: []
        )
        let revealed = Recorder<String>()
        let reveals = listSlot.candidateRevealRequests.sink {
            revealed.record($0)
        }
        defer { reveals.cancel() }
        let requested = Recorder<[String]>()

        await ImportCandidateCombineAction(
            importer: Importer(combineCandidates: { keys in
                requested.record(keys)
                return "combination:new"
            }),
            uiStore: uiStore,
            listSlot: listSlot
        )
        .run()

        // Core orders the folders and names the release, so the only thing that
        // crosses is the selection.
        #expect(requested.all == [["/music/Volume A", "/music/Volume B"]])
        #expect(uiStore.selectedFolderCandidates == ["combination:new"])
        #expect(revealed.all == ["combination:new"])
        #expect(uiStore.lastError == nil)
    }

    @Test("a refused combine keeps the folders selected for another attempt")
    func failureKeepsTheSelection() async {
        let uiStore = UiStore()
        let selected: Set<String> = ["/music/Volume B", "/music/Volume A"]
        uiStore.setFolderCandidateSelection(selected)
        let listSlot = ImportListSlot.preview(
            importStore: ImportStore(),
            uiStore: uiStore,
            items: []
        )
        let revealed = Recorder<String>()
        let reveals = listSlot.candidateRevealRequests.sink {
            revealed.record($0)
        }
        defer { reveals.cancel() }

        let importer = Importer(combineCandidates: { _ in
            throw CombineRefused()
        })
        await ImportCandidateCombineAction(
            importer: importer,
            uiStore: uiStore,
            listSlot: listSlot
        )
        .run()

        #expect(uiStore.selectedFolderCandidates == selected)
        #expect(revealed.all.isEmpty)
        #expect(uiStore.lastError != nil)
    }
}
