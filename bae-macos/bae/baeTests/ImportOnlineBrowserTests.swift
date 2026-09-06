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
}
