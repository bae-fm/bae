import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct ImportCandidateActionRunTests {
    @Test("A submitted batch can be cancelled and rejects a second submission")
    func cancelSubmittedBatch() async throws {
        let candidate = PreviewData.importTabCandidate
        let uiStore = UiStore()
        uiStore.setFolderCandidateSelection([candidate.key])
        let entered = AsyncStream<Void>.makeStream()
        let submitted = uiStore.candidateActionRun.start(
            action: .skip,
            candidates: [candidate],
            uiStore: uiStore,
            before: {},
            operation: { _ in
                entered.continuation.yield(())
                try await Task.sleep(for: .seconds(30))
            }
        )
        let task = try #require(submitted)
        var iterator = entered.stream.makeAsyncIterator()
        await iterator.next()
        let duplicate = uiStore.candidateActionRun.start(
            action: .skip,
            candidates: [candidate],
            uiStore: uiStore,
            before: {},
            operation: { _ in
                Issue.record("A second batch started while one was active")
            }
        )
        #expect(duplicate == nil)
        uiStore.candidateActionRun.cancel()
        await task.value
        entered.continuation.finish()
        #expect(uiStore.selectedFolderCandidates == [candidate.key])
        #expect(!uiStore.candidateActionRun.isRunning)
        #expect(uiStore.lastError == nil)
    }

    @Test("Cancellation retains the cancelled and unattempted folders")
    func cancellationPreservesSelection() async {
        let candidates = [
            PreviewData.importTabCandidate,
            PreviewData.importTabDisagreementCandidate,
            PreviewData.importTabSeveralMatchesCandidate,
        ]
        let uiStore = UiStore()
        uiStore.setFolderCandidateSelection(Set(candidates.map(\.key)))
        var attempted: [String] = []
        await uiStore.candidateActionRun.perform(
            action: .skip,
            candidates: candidates,
            uiStore: uiStore
        ) { key in
            attempted.append(key)
            if key == candidates[1].key { throw CancellationError() }
        }
        #expect(attempted == Array(candidates.prefix(2).map(\.key)))
        #expect(
            uiStore.selectedFolderCandidates
                == Set(candidates.dropFirst().map(\.key))
        )
        #expect(uiStore.lastError == nil)
        #expect(!uiStore.candidateActionRun.isRunning)
    }

    @Test(
        "Metadata commands preserve the selection and report each failed folder"
    )
    func metadataFailuresNameEachFolder() async {
        let candidates = [
            PreviewData.importTabCandidate,
            PreviewData.importTabDisagreementCandidate,
        ]
        let uiStore = UiStore()
        let keys = Set(candidates.map(\.key))
        uiStore.setFolderCandidateSelection(keys)
        var attempted: [String] = []
        await uiStore.candidateActionRun.perform(
            action: .clearMetadata,
            candidates: candidates,
            uiStore: uiStore
        ) { key in
            attempted.append(key)
            throw CocoaError(.fileWriteNoPermission)
        }
        #expect(attempted == candidates.map(\.key))
        #expect(uiStore.selectedFolderCandidates == keys)
        for candidate in candidates {
            #expect(
                uiStore.lastError?.line.contains(candidate.displayName) == true
            )
        }
    }

    @Test("Batch completion preserves folders selected during the operation")
    func selectionChangesSurviveCompletion() async {
        let candidate = PreviewData.importTabCandidate
        let other = PreviewData.importTabDisagreementCandidate
        let uiStore = UiStore()
        uiStore.setFolderCandidateSelection([candidate.key])
        await uiStore.candidateActionRun.perform(
            action: .skip,
            candidates: [candidate],
            uiStore: uiStore
        ) { _ in
            uiStore.setFolderCandidateSelection([other.key])
        }
        #expect(uiStore.selectedFolderCandidates == [other.key])
    }

    @Test("The batch pane renders the real selection's eligible action counts")
    func selectionPaneRenders() async throws {
        let scene = PreviewData.importTabScene()
        let uiStore = UiStore()
        uiStore.setFolderCandidateSelection([
            PreviewData.importTabCandidate.key,
            PreviewData.importTabDisagreementCandidate.key,
        ])
        let selection = ImportCandidateSelection(
            importStore: scene.store,
            uiStore: uiStore
        )
        #expect(selection.candidates(for: .importReady).count == 1)
        #expect(selection.candidates(for: .skip).count == 2)
        #expect(selection.candidates(for: .restore).isEmpty)
        let size = NSSize(width: 720, height: 580)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportCandidateBulkSelectionPane(
                storageCloud: .constant(true),
                storagePinned: .constant(true),
                onPerform: { _ in },
                onCombine: {}
            )
            .environment(scene.store)
            .environment(uiStore)
            .environment(PreviewData.configStore())
            .background(Theme.background)
            .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        #expect(!png.isEmpty)
    }
}
