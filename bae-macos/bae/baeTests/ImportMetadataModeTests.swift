import BaeKit
import Testing

@testable import bae

@MainActor
private final class MetadataModeRecorder {
    var selectedSeeds: [BridgeMetadataSeed] = []
    var previewedKeys: [String] = []
    var identifiedKeys: [String] = []
    var lastModes: [BridgeImportMetadataMode] = []
    var errors: [String] = []
    var previewResult = MappingFixtures.albumSeed
    var previewFailure: (any Error)?

    var importer: Importer {
        Importer(
            selectCandidateMetadataSeed: { [self] _, seed in
                await MainActor.run { selectedSeeds.append(seed) }
            },
            previewFileTags: { [self] key in
                try await MainActor.run {
                    previewedKeys.append(key)
                    if let previewFailure { throw previewFailure }
                    return previewResult
                }
            },
            identifyForExplicitLookup: { [self] key in
                identifiedKeys.append(key)
            },
            setLastMetadataMode: { [self] mode in lastModes.append(mode) }
        )
    }

    func services(_ store: ImportStore) -> ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            importStore: store,
            previewAudio: PreviewAudio.stub(),
            openDocument: { _, _ in },
            openImages: { _, _ in },
            onError: { [self] in errors.append($0) }
        )
    }
}

@MainActor
@Suite("Import metadata modes")
struct ImportMetadataModeTests {
    @Test("an unseeded candidate uses core's resolved metadata mode")
    func unseededCandidateUsesResolvedMetadataMode() throws {
        for mode in [
            BridgeImportMetadataMode.lookup,
            .fileTags,
            .manual,
        ] {
            let store = ImportStore()
            let key = "/watched/\(mode)"

            store.applyCandidateDetail(
                key: key,
                detail: MappingFixtures.detail(
                    mapping: nil,
                    metadataSeed: nil,
                    candidateKey: key,
                    folderName: "Candidate"
                ),
                unseededMetadataMode: mode
            )

            #expect(
                store.selectedCandidates[key]?.presentedMetadataMode == mode
            )
        }
    }

    @Test("a stored seed overrides core's unseeded metadata mode")
    func storedSeedOverridesUnseededMode() throws {
        for (seed, expected) in [
            (BridgeMetadataSeed.fileTags, BridgeImportMetadataMode.fileTags),
            (.manual, .manual),
            (
                .externalRelease(source: .musicBrainz, releaseId: "release"),
                .lookup
            ),
        ] {
            let store = ImportStore()
            store.applyCandidateDetail(
                key: MappingFixtures.candidateKey,
                detail: MappingFixtures.detail(
                    mapping: nil,
                    metadataSeed: seed
                ),
                unseededMetadataMode: .manual
            )

            #expect(
                store.selectedCandidates[MappingFixtures.candidateKey]?
                    .presentedMetadataMode == expected
            )
        }
    }

    @Test(
        "a refresh preserves presentation while a different candidate resolves it"
    )
    func refreshPreservesPresentationAndAnotherCandidateResolves() throws {
        let store = ImportStore()
        let firstKey = "/watched/first"
        let secondKey = "/watched/second"
        store.applyCandidateDetail(
            key: firstKey,
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataSeed: nil,
                candidateKey: firstKey,
                folderName: "First"
            ),
            unseededMetadataMode: .fileTags
        )
        store.presentMetadataMode(.manual, forKey: firstKey)

        store.applyCandidateDetail(
            key: firstKey,
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataSeed: nil,
                candidateKey: firstKey,
                folderName: "First refreshed"
            ),
            unseededMetadataMode: .lookup
        )
        store.applyCandidateDetail(
            key: secondKey,
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataSeed: nil,
                candidateKey: secondKey,
                folderName: "Second"
            ),
            unseededMetadataMode: .fileTags
        )

        #expect(
            store.selectedCandidates[firstKey]?.presentedMetadataMode == .manual
        )
        #expect(
            store.selectedCandidates[secondKey]?.presentedMetadataMode
                == .fileTags
        )
    }
}

extension ImportMetadataModeTests {
    @Test("Lookup navigation preserves the selected seed and identifies")
    func lookupNavigationIdentifiesWithoutWriting() throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.fileTagsTable,
            metadataSeed: .fileTags
        )
        let recorder = MetadataModeRecorder()
        let before = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )

        ImportMappingFlow.presentMetadataMode(
            .lookup,
            for: before,
            services: recorder.services(store)
        )

        let after = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(after.presentedMetadataMode == .lookup)
        #expect(after.metadataSeed == .fileTags)
        #expect(!after.presentedMetadataModeHasSelectedSeed)
        #expect(recorder.selectedSeeds.isEmpty)
        #expect(recorder.previewedKeys.isEmpty)
        #expect(recorder.lastModes == [.lookup])
        #expect(recorder.identifiedKeys == [MappingFixtures.candidateKey])
    }

    @Test("File Tags navigation previews without selecting")
    func fileTagsNavigationPreviewsWithoutSelecting() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataModeRecorder()
        let before = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        let beforeDetail = try #require(before.detail)

        ImportMappingFlow.presentMetadataMode(
            .fileTags,
            for: before,
            services: recorder.services(store)
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .fileTagsPreview.edit != nil
        }

        let previewing = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(previewing.presentedMetadataMode == .fileTags)
        #expect(previewing.metadataSeed == MappingFixtures.seed)
        #expect(previewing.detail == beforeDetail)
        #expect(recorder.previewedKeys == [MappingFixtures.candidateKey])
        #expect(recorder.selectedSeeds.isEmpty)
        #expect(recorder.lastModes == [.fileTags])
        #expect(recorder.identifiedKeys.isEmpty)
    }

    @Test("File Tags selection waits for its exact detail")
    func fileTagsSelectionWaitsForDetail() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataModeRecorder()
        let before = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        let services = recorder.services(store)
        ImportMappingFlow.presentMetadataMode(
            .fileTags,
            for: before,
            services: services
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .fileTagsPreview.edit != nil
        }

        ImportMappingFlow.useFileTags(
            key: MappingFixtures.candidateKey,
            services: services
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataSeedSession?
                .commandSucceeded == true
        }
        #expect(recorder.selectedSeeds == [.fileTags])
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .seedInFlight == .fileTags
        )

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.fileTagsTable,
                metadataSeed: .fileTags
            )
        )
        let selected = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(selected.seedInFlight == nil)
        #expect(selected.presentedMetadataModeHasSelectedSeed)
    }

    @Test("Manual navigation is blank until explicit selection")
    func manualNavigationDoesNotWrite() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataModeRecorder()
        let before = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        let services = recorder.services(store)

        ImportMappingFlow.presentMetadataMode(
            .manual,
            for: before,
            services: services
        )
        #expect(recorder.selectedSeeds.isEmpty)
        #expect(recorder.previewedKeys.isEmpty)
        #expect(recorder.lastModes == [.manual])
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .presentedMetadataMode == .manual
        )

        ImportMappingFlow.enterManually(
            key: MappingFixtures.candidateKey,
            services: services
        )
        await waitUntil { recorder.selectedSeeds == [.manual] }
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .seedInFlight == .manual
        )
    }

    @Test("a failed File Tags preview remains visible and surfaces the error")
    func failedFileTagsPreviewRemainsVisible() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataModeRecorder()
        recorder.previewFailure = StubError.notImplemented
        let before = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )

        ImportMappingFlow.presentMetadataMode(
            .fileTags,
            for: before,
            services: recorder.services(store)
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .fileTagsPreview == .failed
        }

        let after = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(after.presentedMetadataMode == .fileTags)
        #expect(after.error != nil)
        #expect(after.metadataSeed == MappingFixtures.seed)
        #expect(recorder.lastModes == [.fileTags])

        ImportMappingFlow.loadFileTagsPreview(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        await waitUntil {
            recorder.previewedKeys.count == 2
        }
        #expect(recorder.lastModes == [.fileTags])
    }

    @Test("explicit Lookup starts interactive identification when unseeded")
    func explicitLookupStartsIdentification() throws {
        let store = MappingFixtures.store(mapping: nil, metadataSeed: nil)
        let recorder = MetadataModeRecorder()
        let candidate = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )

        ImportMappingFlow.presentMetadataMode(
            .lookup,
            for: candidate,
            services: recorder.services(store)
        )

        #expect(recorder.lastModes == [.lookup])
        #expect(recorder.identifiedKeys == [MappingFixtures.candidateKey])
    }

    private func waitUntil(_ predicate: () -> Bool) async {
        for _ in 0..<100 where !predicate() {
            await Task.yield()
        }
        #expect(predicate())
    }
}
