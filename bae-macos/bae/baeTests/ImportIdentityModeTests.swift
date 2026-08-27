import BaeKit
import Testing

@testable import bae

@MainActor
private final class IdentityModeRecorder {
    var picks: [BridgeIdentityPick] = []

    var importer: Importer {
        Importer(
            pickCandidateIdentity: { [self] _, pick in
                await MainActor.run { picks.append(pick) }
            }
        )
    }

    func services(_ store: ImportStore) -> ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            importStore: store,
            previewAudio: PreviewAudio.stub(),
            openDocument: { _, _ in },
            openImages: { _, _ in },
            onError: { _ in }
        )
    }
}

@MainActor
@Suite("Import identity mode")
struct ImportIdentityModeTests {
    @Test("Lookup presents inline without starting a pick")
    func lookupPresentsInlineWithoutStartingAPick() throws {
        let store = ImportStore()
        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.unknownTable,
                picked: .unknown
            )
        )
        let recorder = IdentityModeRecorder()
        let pendingPick = BridgeIdentityPick.release(
            source: .musicBrainz,
            releaseId: "rel-loading"
        )
        store.mutateCandidate(forKey: MappingFixtures.candidateKey) {
            $0.identityPickSession = CandidateIdentityPickSession(
                pick: pendingPick
            )
            $0.search.searchAlbum = "typed album"
        }
        let before = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )

        ImportMappingFlow.setIdentity(
            .release,
            for: before,
            services: recorder.services(store)
        )

        let after = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(after.presentedIdentity == .release)
        #expect(after.identity == .unknown)
        #expect(!after.presentedIdentityHasSettled)
        #expect(after.pickInFlight == pendingPick)
        #expect(after.search.searchAlbum == "typed album")
        #expect(recorder.picks.isEmpty)
    }

    @Test("File Tags preserves the settled release while its pick is pending")
    func fileTagsPreservesSettledReleaseWhilePending() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = IdentityModeRecorder()
        let before = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        let beforeDetail = try #require(before.detail)
        #expect(before.presentedIdentityHasSettled)

        ImportMappingFlow.setIdentity(
            .unknown,
            for: before,
            services: recorder.services(store)
        )

        let pending = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(pending.presentedIdentity == .unknown)
        #expect(!pending.presentedIdentityHasSettled)
        #expect(pending.pickedRelease?.releaseId == MappingFixtures.releaseId)
        #expect(pending.detail == beforeDetail)
        #expect(pending.pickInFlight == .unknown)

        try await Task.sleep(for: .milliseconds(50))
        #expect(recorder.picks == [.unknown])
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .pickInFlight == nil
        )
    }

    @Test("File Tags omits release search controls")
    func fileTagsOmitsReleaseSearchControls() {
        #expect(
            ImportReleaseSearchControl(identity: .unknown, hasPick: false)
                == nil
        )
        #expect(
            ImportReleaseSearchControl(identity: .unknown, hasPick: true)
                == nil
        )
        #expect(
            ImportReleaseSearchControl(identity: .release, hasPick: false)
                == .find
        )
        #expect(
            ImportReleaseSearchControl(identity: .release, hasPick: true)
                == .change
        )
    }

    @Test("a failed File Tags pick returns to the stored identity")
    func failedFileTagsPickRestoresStoredIdentity() async throws {
        let store = ImportStore()
        let candidate = PreviewData.folderCandidates[0]
        store.selectedCandidates[candidate.key] = candidate
        let importer = Importer(
            pickCandidateIdentity: { _, _ in throw StubError.notImplemented }
        )
        let services = ImportMappingServices(
            importer: importer,
            importStore: store,
            previewAudio: PreviewAudio.stub(),
            openDocument: { _, _ in },
            openImages: { _, _ in },
            onError: { _ in }
        )

        ImportMappingFlow.setIdentity(
            .unknown,
            for: candidate,
            services: services
        )
        try await Task.sleep(for: .milliseconds(50))

        let after = try #require(store.candidate(forKey: candidate.key))
        #expect(after.error != nil)
        #expect(after.pickInFlight == nil)
        #expect(after.presentedIdentity == after.identity)
    }
}
