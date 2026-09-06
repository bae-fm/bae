import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

@MainActor
struct ImportSourceSearchTests {
    @Test
    func automaticIdentificationKeepsTheOpenResultsVisible() async throws {
        let store = ImportStore()
        let key = MappingFixtures.candidateKey
        store.applyCandidateDetail(
            key: key,
            detail: MappingFixtures.detail(
                mapping: nil,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil,
                presentation: .findOnline
            )
        )
        let candidate = try #require(store.candidate(forKey: key))
        var presentations: [CandidateMetadataPresentation] = []
        let view = ImportMetadataSourceSection(
            candidate: candidate,
            runtime: nil,
            fileTagsPreviewSummary: nil,
            isReading: false,
            coverContent: nil,
            hasCoverOptions: false,
            editActions: ReleaseFieldWriter { _, _ in },
            editingCommands: EditingCommitCommands(),
            endEditing: {},
            commit: nil,
            onPresent: { presentations.append($0) },
            onReadFileTags: {},
            onUseFileTags: {},
            onClearMetadata: {},
            onEditCover: {},
            onSelectCover: { _ in }
        )
        .environment(store)
        .environment(Importer.stub())
        .importPreviewEnvironment()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            view,
            size: NSSize(width: 800, height: 500)
        )
        window.isReleasedWhenClosed = false
        defer { window.close() }
        await SnapshotTestSupport.settle(host)
        store.applyCandidateDetail(
            key: key,
            detail: MappingFixtures.detail(
                mapping: nil,
                presentation: .findOnline
            )
        )
        await SnapshotTestSupport.settle(host)

        #expect(presentations.isEmpty)
    }

    @Test(arguments: [BridgeMetadataSource.musicBrainz, .discogs])
    func sourceSearchKeepsSelectedMetadata(source: BridgeMetadataSource) throws
    {
        let store = MappingFixtures.store(mapping: nil)
        let before = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)?.detail
        )
        let recorder = SourceSearchRecorder()
        let importer = Importer(startSourceCandidateSearch: {
            key,
            query,
            source in
            recorder.record(key: key, query: query, source: source)
        })

        ImportSearchFlow.searchRelease(
            services: ImportSearchFlow.ImportServices(
                importer: importer,
                importStore: store
            ),
            key: MappingFixtures.candidateKey,
            artist: "Artist Name",
            title: "Album Title",
            source: source
        )

        let request = try #require(recorder.request)
        #expect(request.key == MappingFixtures.candidateKey)
        #expect(
            request.query
                == .general(artist: "Artist Name", album: "Album Title")
        )
        #expect(request.source == source)
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?.detail
                == before
        )
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataApplicationSession == nil
        )
    }
}

private final class SourceSearchRecorder: @unchecked Sendable {
    struct Request {
        let key: String
        let query: BridgeSearchQuery
        let source: BridgeMetadataSource
    }

    private let lock = NSLock()
    private var recorded: Request?

    var request: Request? {
        lock.withLock { recorded }
    }

    func record(
        key: String,
        query: BridgeSearchQuery,
        source: BridgeMetadataSource
    ) {
        lock.withLock {
            recorded = Request(key: key, query: query, source: source)
        }
    }
}
