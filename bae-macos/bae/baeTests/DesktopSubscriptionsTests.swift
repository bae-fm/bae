import BaeKit
import Testing

@testable import bae

@MainActor
@Suite("Import candidate selection")
struct DesktopSubscriptionsTests {
    @Test("one read follows the one selected candidate, and ends without one")
    func oneReadFollowsTheSingleSelection() async throws {
        let feed = DetailFeed<BridgeImportCandidateDetail>()
        let observation = ImportSelectionObservation(
            open: { feed.query() },
            importStore: ImportStore(),
            uiStore: UiStore()
        )

        observation.selectionChanged(single: "candidate-a")
        observation.selectionChanged(single: "candidate-b")
        #expect(feed.opened == 1)
        #expect(feed.requested == ["candidate-a", "candidate-b"])

        observation.selectionChanged(single: nil)
        try await Wait.until { feed.isCancelled(read: 0) }
        #expect(feed.isCancelled(read: 0), "no single selection, no read")
    }

    /// A pick is about a folder. When the read says there is no such folder
    /// any more, there is nothing left for the pick to claim.
    @Test("a folder that is gone cancels the pick made on it")
    func aGoneFolderCancelsThePick() async throws {
        let key = MappingFixtures.candidateKey
        let feed = DetailFeed<BridgeImportCandidateDetail>()
        let store = ImportStore()
        let observation = ImportSelectionObservation(
            open: { feed.query() },
            importStore: store,
            uiStore: UiStore()
        )

        observation.selectionChanged(single: key)
        feed.emit(id: key, value: MappingFixtures.detail(mapping: nil))
        try await Wait.until { !store.selectedCandidates.isEmpty }
        _ = try #require(
            store.beginMetadataApplication(
                key: key,
                provenance: MappingFixtures.provenance
            )
        )

        feed.emit(id: key, value: nil)
        try await Wait.until {
            store.metadataApplicationSession(forKey: key) == nil
        }

        #expect(store.metadataApplicationSession(forKey: key) == nil)
    }
}
