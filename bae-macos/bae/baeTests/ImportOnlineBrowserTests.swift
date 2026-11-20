import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

@MainActor
struct ImportOnlineBrowserTests {
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
                releaseLink: nil,
                presentation: .findOnline
            )
        )
        let candidate = try #require(store.candidate(forKey: key))
        var moves: [PaneMove] = []
        let view = ImportOnlineMetadataBrowser(
            candidateKey: candidate.key,
            endEditing: {},
            onMovePane: { moves.append($0) }
        )
        .environment(store)
        .environment(Importer.stub())
        .importPreviewEnvironment()
        try await SnapshotTestSupport.withHostedWindow(
            view,
            size: NSSize(width: 800, height: 500)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            store.applyCandidateDetail(
                key: key,
                detail: MappingFixtures.detail(
                    mapping: nil,
                    presentation: .findOnline
                )
            )
            try await SnapshotTestSupport.settle(host)

            #expect(moves.isEmpty)
        }
    }
}
