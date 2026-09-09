import BaeKit
import Testing

@testable import bae

/// Picking a pressing is a decision about one folder. The store holds it under
/// that folder's key, so looking at another candidate — or at none — neither
/// cancels the read nor loses where it lands.
@MainActor
@Suite("A pick outlives the selection")
struct ImportStorePickTests {
    private static let key = MappingFixtures.candidateKey
    private static let audio = "scanned-audio-a"

    @Test("deselecting the candidate leaves its pick in flight")
    func deselectingLeavesThePickInFlight() throws {
        let store = pickingStore()
        let session = try #require(beginPick(on: store))

        store.selectedCandidates.removeValue(forKey: Self.key)

        #expect(store.metadataApplicationSession(forKey: Self.key) === session)
        #expect(
            store.loadingReleaseId(forKey: Self.key)
                == MappingFixtures.releaseId
        )
    }

    @Test("re-reading the same folder finds the pick still in flight")
    func reReadingFindsThePickInFlight() throws {
        let store = pickingStore()
        let session = try #require(beginPick(on: store))
        store.selectedCandidates.removeValue(forKey: Self.key)

        store.applyCandidateDetail(key: Self.key, detail: read())

        #expect(store.metadataApplicationSession(forKey: Self.key) === session)
    }

    @Test("a pick that lands with nothing selected writes the pane to draft")
    func landingWithNothingSelectedWritesTheDraft() async throws {
        let writes = SessionWriteRecorder()
        let store = pickingStore(writes: writes)
        let session = try #require(beginPick(on: store))
        store.selectedCandidates.removeValue(forKey: Self.key)

        store.metadataApplicationSucceeded(key: Self.key, session: session)

        await waitUntil {
            writes.presentations(forKey: Self.key) == [.draft]
        }
        #expect(store.metadataApplicationSession(forKey: Self.key) == nil)
        #expect(store.loadingReleaseId(forKey: Self.key) == nil)
    }

    @Test("a pick that fails with nothing selected keeps its pressing's line")
    func failingWithNothingSelectedKeepsItsLine() throws {
        let store = pickingStore()
        let session = try #require(beginPick(on: store))
        store.selectedCandidates.removeValue(forKey: Self.key)

        store.metadataApplicationFailed(
            key: Self.key,
            session: session,
            error: "Release details unavailable"
        )

        #expect(store.metadataApplicationSession(forKey: Self.key) == nil)
        #expect(
            store.releaseSelectionFailure(forKey: Self.key)?.release.releaseId
                == MappingFixtures.releaseId
        )
    }

    @Test("audio that changed under the pick cancels it")
    func changedAudioCancelsThePick() throws {
        let store = pickingStore()
        _ = try #require(beginPick(on: store))

        store.applyCandidateDetail(
            key: Self.key,
            detail: read(audio: "scanned-audio-b")
        )

        #expect(store.metadataApplicationSession(forKey: Self.key) == nil)
    }

    @Test("the candidate leaving the list cancels its pick")
    func aRemovedCandidateCancelsThePick() throws {
        let store = pickingStore()
        _ = try #require(beginPick(on: store))

        store.cancelMetadataApplication(forKey: Self.key)

        #expect(store.metadataApplicationSession(forKey: Self.key) == nil)
    }

    private func pickingStore(
        writes: SessionWriteRecorder? = nil
    ) -> ImportStore {
        let store = ImportStore()
        if let writes {
            store.sessionWriter = .recording { writes.record($0) }
        }
        store.applyCandidateDetail(key: Self.key, detail: read())
        return store
    }

    private func beginPick(
        on store: ImportStore
    ) -> CandidateMetadataApplicationSession? {
        store.beginMetadataApplication(
            key: Self.key,
            provenance: MappingFixtures.provenance
        )
    }

    private func read(
        audio: String = ImportStorePickTests.audio
    ) -> BridgeImportCandidateDetail {
        MappingFixtures.detail(mapping: nil, audioIdentity: audio)
    }

    private func waitUntil(_ predicate: () -> Bool) async {
        for _ in 0..<100 where !predicate() {
            await Task.yield()
        }
        #expect(predicate())
    }
}
