import BaeKit
import Testing

@testable import bae

@MainActor
@Suite("Import candidate selection")
struct DesktopSubscriptionsTests {
    @Test("moving the selection moves its reads, and only growth opens more")
    func selectionMovesItsReads() async {
        let feed = DetailFeed<BridgeImportCandidateDetail>()
        let observations = ImportSelectionObservations(
            open: { feed.query() },
            importStore: ImportStore(),
            uiStore: UiStore()
        )

        observations.selectionChanged(["candidate-a"])
        observations.selectionChanged(["candidate-b"])
        #expect(feed.opened == 1)
        #expect(feed.requested == ["candidate-a", "candidate-b"])

        observations.selectionChanged(["candidate-b", "candidate-c"])
        #expect(feed.opened == 2)

        observations.selectionChanged(["candidate-c"])
        await waitForStoreUpdate { feed.isCancelled(read: 0) }
        #expect(
            feed.isCancelled(read: 0),
            "the read a smaller selection frees ends"
        )
        #expect(!feed.isCancelled(read: 1))
    }

    /// A pick is about a folder. When the read says there is no such folder
    /// any more, there is nothing left for the pick to claim.
    @Test("a folder that is gone cancels the pick made on it")
    func aGoneFolderCancelsThePick() async throws {
        let key = MappingFixtures.candidateKey
        let feed = DetailFeed<BridgeImportCandidateDetail>()
        let store = ImportStore()
        let observations = ImportSelectionObservations(
            open: { feed.query() },
            importStore: store,
            uiStore: UiStore()
        )

        observations.selectionChanged([key])
        feed.emit(id: key, value: MappingFixtures.detail(mapping: nil))
        await waitForStoreUpdate { !store.selectedCandidates.isEmpty }
        _ = try #require(
            store.beginMetadataApplication(
                key: key,
                provenance: MappingFixtures.provenance
            )
        )

        feed.emit(id: key, value: nil)
        await waitForStoreUpdate {
            store.metadataApplicationSession(forKey: key) == nil
        }

        #expect(store.metadataApplicationSession(forKey: key) == nil)
    }
}
