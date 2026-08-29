import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

private struct ExternalMetadataApplication: Equatable {
    let key: String
    let source: BridgeMetadataSource
    let releaseId: String
}

@MainActor
private final class MetadataSourceRecorder {
    var externalApplications: [ExternalMetadataApplication] = []
    var fileTagApplications: [String] = []
    var clearedKeys: [String] = []
    var previewedKeys: [String] = []
    var identifiedKeys: [String] = []
    var errors: [String] = []
    var previewResult = MappingFixtures.albumSeed
    var previewFailure: (any Error)?

    var importer: Importer {
        Importer(
            applyCandidateExternalMetadata: { [self] key, source, releaseId in
                await MainActor.run {
                    externalApplications.append(
                        ExternalMetadataApplication(
                            key: key,
                            source: source,
                            releaseId: releaseId
                        )
                    )
                }
                return 1
            },
            applyCandidateFileTags: { [self] key in
                await MainActor.run { fileTagApplications.append(key) }
                return 1
            },
            clearCandidateMetadata: { [self] key in
                await MainActor.run { clearedKeys.append(key) }
                return 1
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
            }
        )
    }

    func services(
        _ store: ImportStore,
        automaticIdentification: Bool = true
    ) -> ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            automaticIdentification: automaticIdentification,
            importStore: store,
            previewAudio: PreviewAudio.stub(),
            openDocument: { _, _ in },
            openImages: { _, _ in },
            onError: { [self] in errors.append($0) }
        )
    }
}

@MainActor
@Suite("Import metadata sources")
struct ImportMetadataSourceTests {}

extension ImportMetadataSourceTests {
    @Test("an applied draft names both replacement sources directly")
    func appliedDraftNamesReplacementSources() async {
        let view = ImportReleaseHeader(
            releaseSummary: ImportReleaseSummary(
                candidate: PreviewData.mappingCandidate,
                editValues: PreviewData.confirmEditValues
            ),
            draftIsBlank: false,
            isReading: false,
            coverContent: nil,
            hasCoverOptions: false,
            editValues: PreviewData.confirmEditValues,
            editActions: ReleaseFieldWriter { _, _ in },
            commit: nil,
            sourceActions: ImportReleaseSourceActions(
                findOnline: {},
                useFileTags: {},
                clearMetadata: {}
            ),
            localCoverSelections: [:],
            onEditCover: {},
            onSelectCover: { _ in }
        )
        .frame(width: 900, height: 360)
        .importPreviewEnvironment()
        .environment(Library.stub())

        let size = NSSize(width: 900, height: 360)
        let (window, host) = SnapshotTestSupport.hostInWindow(view, size: size)
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let labels = SnapshotTestSupport.descendants(of: host)
            .compactMap { ($0 as? NSButton)?.title }

        #expect(labels.contains(String(localized: "Find online…")))
        #expect(
            labels.contains(
                coreString("ui.import.metadata.file_tags") + "…"
            )
        )
        withExtendedLifetime(window) {}
    }

    @Test("an unpopulated draft opens its configured source")
    func unpopulatedDraftOpensConfiguredSource() throws {
        for (source, expected) in [
            (
                BridgeDefaultImportMetadataSource.findOnline,
                CandidateMetadataPresentation.findOnline
            ),
            (.fileTags, .fileTags),
            (.none, .draft),
        ] {
            let detail = MappingFixtures.detail(
                mapping: nil,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil,
                initialMetadataSource: source
            )

            #expect(Candidate(detail: detail).metadataPresentation == expected)
        }
    }

    @Test("an applied source opens the editable draft")
    func appliedSourceOpensDraft() {
        for provenance in [
            BridgeMetadataProvenance.fileTags,
            .externalRelease(source: .musicBrainz, releaseId: "release"),
        ] {
            let detail = MappingFixtures.detail(
                mapping: nil,
                metadataProvenance: provenance,
                initialMetadataSource: .findOnline
            )

            #expect(
                Candidate(detail: detail).metadataPresentation == .draft
            )
        }
    }

    @Test("a detail refresh preserves the open source browser")
    func detailRefreshPreservesPresentation() throws {
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            edit: MappingFixtures.blankEdit,
            initialMetadataSource: .none
        )
        store.presentMetadata(
            .fileTags,
            forKey: MappingFixtures.candidateKey
        )

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: nil,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil,
                initialMetadataSource: .findOnline
            )
        )

        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataPresentation == .fileTags
        )
    }

    @Test("Find online identifies only when automatic identification is on")
    func findOnlineHonorsAutomaticIdentification() throws {
        for enabled in [false, true] {
            let store = MappingFixtures.store(
                mapping: nil,
                metadataProvenance: nil,
                edit: MappingFixtures.blankEdit
            )
            let recorder = MetadataSourceRecorder()
            let candidate = try #require(
                store.candidate(forKey: MappingFixtures.candidateKey)
            )

            ImportMappingFlow.presentMetadata(
                .findOnline,
                for: candidate,
                services: recorder.services(
                    store,
                    automaticIdentification: enabled
                )
            )

            #expect(
                recorder.identifiedKeys
                    == (enabled ? [MappingFixtures.candidateKey] : [])
            )
        }
    }

    @Test("File tags are previewed without changing the draft")
    func fileTagsPreviewDoesNotApply() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataSourceRecorder()
        let before = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        let beforeDetail = try #require(before.detail)

        ImportMappingFlow.presentMetadata(
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
        #expect(previewing.metadataPresentation == .fileTags)
        #expect(previewing.metadataProvenance == MappingFixtures.provenance)
        #expect(previewing.detail == beforeDetail)
        #expect(recorder.previewedKeys == [MappingFixtures.candidateKey])
        #expect(recorder.fileTagApplications.isEmpty)
    }

    @Test("applying File tags waits for its authoritative detail")
    func fileTagsApplicationWaitsForDetail() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataSourceRecorder()
        let services = recorder.services(store)
        let candidate = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        ImportMappingFlow.presentMetadata(
            .fileTags,
            for: candidate,
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
                .metadataApplicationSession?
                .commandRevision == 1
        }
        #expect(recorder.fileTagApplications == [MappingFixtures.candidateKey])
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataPresentation == .fileTags
        )

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.fileTagsTable,
                metadataProvenance: .fileTags
            )
        )

        let applied = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(applied.metadataApplicationSession == nil)
        #expect(applied.metadataPresentation == .draft)
        #expect(applied.metadataProvenance == .fileTags)
    }

    @Test("applying an online result waits for the matching release detail")
    func onlineApplicationWaitsForMatchingDetail() async throws {
        let key = MappingFixtures.candidateKey
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            edit: MappingFixtures.blankEdit
        )
        let recorder = MetadataSourceRecorder()
        store.presentMetadata(.findOnline, forKey: key)

        ImportSearchFlow.applyMetadata(
            importer: recorder.importer,
            importStore: store,
            key: key,
            provenance: MappingFixtures.provenance,
            onConfirmed: {
                Task { @MainActor in
                    store.presentMetadata(.draft, forKey: key)
                }
            }
        )
        await waitUntil {
            store.candidate(forKey: key)?
                .metadataApplicationSession?
                .commandRevision == 1
        }
        #expect(
            recorder.externalApplications.map(\.key) == [key]
        )
        #expect(
            store.candidate(forKey: key)?
                .metadataPresentation == .findOnline
        )

        store.applyCandidateDetail(
            key: key,
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataProvenance: .externalRelease(
                    source: .musicBrainz,
                    releaseId: "different"
                )
            )
        )
        #expect(
            store.candidate(forKey: key)?
                .metadataApplicationSession != nil
        )

        store.applyCandidateDetail(
            key: key,
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataProvenance: MappingFixtures.provenance
            )
        )
        await waitUntil {
            store.candidate(forKey: key)?
                .metadataPresentation == .draft
        }
        #expect(
            store.candidate(forKey: key)?
                .metadataApplicationSession == nil
        )
    }

    @Test("clearing metadata dispatches the candidate command")
    func clearMetadataDispatchesCommand() async {
        let store = MappingFixtures.store(mapping: nil)
        let recorder = MetadataSourceRecorder()

        ImportMappingFlow.clearMetadata(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        await waitUntil { !recorder.clearedKeys.isEmpty }

        #expect(recorder.clearedKeys == [MappingFixtures.candidateKey])
        #expect(recorder.errors.isEmpty)
    }

    @Test("a failed File tags preview remains retryable")
    func failedFileTagsPreviewRemainsRetryable() async throws {
        let store = MappingFixtures.store(mapping: nil)
        let recorder = MetadataSourceRecorder()
        recorder.previewFailure = StubError.notImplemented
        let candidate = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )

        ImportMappingFlow.presentMetadata(
            .fileTags,
            for: candidate,
            services: recorder.services(store)
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .fileTagsPreview == .failed
        }

        ImportMappingFlow.loadFileTagsPreview(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        await waitUntil { recorder.previewedKeys.count == 2 }
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataPresentation == .fileTags
        )
    }

    private func waitUntil(_ predicate: () -> Bool) async {
        for _ in 0..<100 where !predicate() {
            await Task.yield()
        }
        #expect(predicate())
    }
}
