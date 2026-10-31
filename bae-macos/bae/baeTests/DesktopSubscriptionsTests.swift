import BaeKit
import Foundation
import Testing

@testable import bae

@MainActor
@Suite("Import candidate selection")
struct DesktopSubscriptionsTests {
    @Test("one read follows the one selected candidate, and ends without one")
    func oneReadFollowsTheSingleSelection() async throws {
        let feed = KeyedDetailFeed<BridgeImportCandidateDetail, Data>()
        let observation = ImportSelectionObservation(
            open: { feed.query() },
            importStore: ImportStore(),
            uiStore: UiStore()
        )

        observation.selectionChanged(single: Data("candidate-a".utf8))
        observation.selectionChanged(single: Data("candidate-b".utf8))
        #expect(feed.opened == 1)
        #expect(
            feed.requested == [
                Data("candidate-a".utf8), Data("candidate-b".utf8),
            ]
        )

        observation.selectionChanged(single: nil)
        try await Wait.until { feed.isCancelled(read: 0) }
        #expect(feed.isCancelled(read: 0), "no single selection, no read")
    }

    /// Each of the list's values carries the selection, so the one selected
    /// candidate arrives again with every value the pane's edits cause: the
    /// pane keeps the candidate it shows and the read behind it.
    @Test("the same selected candidate again keeps the pane's candidate")
    func theSameSelectionAgainKeepsThePane() async throws {
        let key = MappingFixtures.candidateKey
        let feed = KeyedDetailFeed<BridgeImportCandidateDetail, Data>()
        let store = ImportStore()
        let observation = ImportSelectionObservation(
            open: { feed.query() },
            importStore: store,
            uiStore: UiStore()
        )
        observation.selectionChanged(single: key)
        feed.emit(id: key, value: MappingFixtures.detail(mapping: nil))
        try await Wait.until { store.selectedCandidates[key] != nil }

        observation.selectionChanged(single: key)

        #expect(store.selectedCandidates[key] != nil)
        #expect(feed.opened == 1)
        #expect(feed.requested == [key])
        #expect(!feed.isCancelled(read: 0))
    }

    /// A pick is about a folder. When the read says there is no such folder
    /// any more, there is nothing left for the pick to claim.
    @Test("a folder that is gone cancels the pick made on it")
    func aGoneFolderCancelsThePick() async throws {
        let key = MappingFixtures.candidateKey
        let feed = KeyedDetailFeed<BridgeImportCandidateDetail, Data>()
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
                application: .pick(MappingFixtures.link)
            )
        )

        feed.emit(id: key, value: nil)
        try await Wait.until {
            store.metadataApplicationSession(forKey: key) == nil
        }

        #expect(store.metadataApplicationSession(forKey: key) == nil)
    }
}
