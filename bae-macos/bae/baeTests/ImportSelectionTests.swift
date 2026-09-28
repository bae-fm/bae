import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

/// Values a test's stand-in core records from any thread.
final class CallLog<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [Value] = []

    var all: [Value] {
        lock.withLock { values }
    }

    func record(_ value: Value) {
        lock.withLock { values.append(value) }
    }
}

extension ImportSelectionOperations {
    /// The inert operations with the ones a test watches replaced.
    static func stub(
        change:
            @escaping @Sendable (BridgeImportListView, BridgeSelectionChange)
            async throws -> UInt64 = { _, _ in 0 },
        selectAll:
            @escaping @Sendable (BridgeImportListView) async throws -> Void = {
                _ in
            },
        keepShown:
            @escaping @Sendable (BridgeImportListView) async throws -> Void = {
                _ in
            },
        combine: @escaping @Sendable () async throws -> String = {
            throw StubError.notImplemented
        },
        run:
            @escaping @Sendable (
                BridgeImportListView,
                BridgeCandidateAction,
                @escaping @Sendable (BridgeSelectionActionProgress) -> Void
            ) async throws -> [BridgeSelectionActionFailure] = { _, _, _ in [] }
    ) -> ImportSelectionOperations {
        ImportSelectionOperations(
            change: change,
            selectAll: selectAll,
            keepShown: keepShown,
            sourceFolders: inert.sourceFolders,
            combine: combine,
            run: run
        )
    }
}

extension BridgeImportListView {
    /// Pending, unfiltered, sorted by path.
    static let sortedByPath = BridgeImportListView(
        tab: .pending,
        filterText: "",
        pendingFilter: nil,
        collapsedGroups: [],
        order: .pathAscending
    )
}

@MainActor
struct ImportSelectionTests {
    @Test("A running action can be cancelled and rejects a second one")
    func cancelRunningAction() async throws {
        let entered = AsyncStream<Void>.makeStream()
        let selection = ImportSelection(
            operations: .stub(run: { _, _, _ in
                entered.continuation.yield(())
                try await Task.sleep(for: .seconds(30))
                return []
            })
        )
        let uiStore = UiStore()
        let started = selection.start(
            .skip,
            in: .sortedByPath,
            uiStore: uiStore,
            before: {}
        )
        let task = try #require(started)
        var iterator = entered.stream.makeAsyncIterator()
        await iterator.next()
        let second = selection.start(
            .skip,
            in: .sortedByPath,
            uiStore: uiStore,
            before: {}
        )
        #expect(second == nil)
        selection.cancel()
        await task.value
        entered.continuation.finish()
        #expect(!selection.isRunning)
        #expect(uiStore.lastError == nil)
    }

    @Test(
        "The action runs in core in the list's order, and each row it failed on is named"
    )
    func failuresNameEachRow() async {
        let requested = CallLog<BridgeCandidateAction>()
        let views = CallLog<BridgeImportListView>()
        let selection = ImportSelection(
            operations: .stub(run: { view, action, _ in
                requested.record(action)
                views.record(view)
                return [
                    BridgeSelectionActionFailure(
                        candidateKey: "/music/Album",
                        name: "Album",
                        error: .Diagnostic(
                            category: .candidateBeingIdentified,
                            detail: "still being identified"
                        )
                    ),
                    BridgeSelectionActionFailure(
                        candidateKey: "/music/Other Album",
                        name: "Other Album",
                        error: .Diagnostic(
                            category: .candidateBeingIdentified,
                            detail: "still being identified"
                        )
                    ),
                ]
            })
        )
        let uiStore = UiStore()
        await selection.perform(.import, in: .sortedByPath, uiStore: uiStore)

        #expect(requested.all == [.import])
        #expect(views.all == [.sortedByPath])
        let line = uiStore.lastError?.line ?? ""
        #expect(line.contains("Album"))
        #expect(line.contains("Other Album"))
        #expect(
            line.contains(
                coreString("core.import.error.candidate_being_identified")
            )
        )
        #expect(!selection.isRunning)
    }

    @Test("Core's progress reaches the pane while the action runs")
    func progressFollowsCore() async throws {
        let release = AsyncStream<Void>.makeStream()
        let selection = ImportSelection(
            operations: .stub(run: { _, _, progress in
                progress(BridgeSelectionActionProgress(completed: 1, total: 3))
                for await _ in release.stream { break }
                return []
            })
        )
        let started = selection.start(
            .skip,
            in: .sortedByPath,
            uiStore: UiStore(),
            before: {}
        )
        let task = try #require(started)
        try await Wait.until { selection.progress?.completed == 1 }
        #expect(selection.progress?.total == 3)
        release.continuation.yield(())
        await task.value
        #expect(selection.progress == nil)
    }

    @Test("The pane draws the selection's offers with their counts")
    func selectionPaneRenders() async throws {
        let selection = PreviewData.importSelection(
            of: [
                PreviewData.importTabCandidate.key,
                PreviewData.importTabDisagreementCandidate.key,
            ],
            in: PreviewData.importTabScene().store
        )
        let counts = Dictionary(
            uniqueKeysWithValues:
                ImportCandidateActionOffer.selection(
                    selection.summary
                )
                .map { ($0.action, $0.count) }
        )
        #expect(counts[.import] == 1)
        #expect(counts[.skip] == 2)
        #expect(counts[.restore] == nil)
        #expect(counts[.combine] == .some(nil))
        #expect(counts[.revealFolder] == 2)
        let size = NSSize(width: 720, height: 580)
        try await SnapshotTestSupport.withHostedWindow(
            ImportCandidateBulkSelectionPane(
                storageCloud: .constant(true),
                storagePinned: .constant(true),
                onPerform: { _ in }
            )
            .environment(selection)
            .environment(PreviewData.configStore())
            .background(Theme.background)
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            #expect(!png.isEmpty)
        }
    }
}

/// What the list shows selected between a click and core's list catching up.
@MainActor
@Suite("A click's selection until core's list reflects it")
struct PendingSelectionClickTests {
    private static func store(selecting key: String?) -> ImportStore {
        let store = ImportStore()
        store.ingest(
            ["/music/Album", "/music/Other Album"]
                .map { path in
                    var row = PreviewData.triageRowIdentified
                    row.candidateKey = path
                    row.selected = path == key
                    return PreviewData.candidateItem(row)
                }
        )
        return store
    }

    @Test("an older list read leaves the click shown until its revision lands")
    func olderReadsLeaveTheClick() {
        let store = Self.store(selecting: "/music/Album")
        store.applySelectionRevision(4)
        let click = store.beginSelectionClick(["/music/Other Album"])

        store.applySelectionRevision(4)
        #expect(store.shownSelectedKeys == ["/music/Other Album"])
        store.selectionClickWritten(click, revision: 5)
        store.applySelectionRevision(4)
        #expect(store.shownSelectedKeys == ["/music/Other Album"])

        store.applySelectionRevision(5)
        #expect(store.shownSelectedKeys == ["/music/Album"])
    }

    @Test("a list read that landed before the write answered clears the click")
    func aReadAheadOfTheAnswerClears() {
        let store = Self.store(selecting: "/music/Album")
        let click = store.beginSelectionClick(["/music/Other Album"])
        store.applySelectionRevision(7)
        #expect(store.shownSelectedKeys == ["/music/Other Album"])

        store.selectionClickWritten(click, revision: 7)
        #expect(store.shownSelectedKeys == ["/music/Album"])
    }

    @Test("a failed write shows core's rows, and only the latest click counts")
    func failuresAndNewerClicks() {
        let store = Self.store(selecting: "/music/Album")
        let first = store.beginSelectionClick(["/music/Other Album"])
        let second = store.beginSelectionClick([])

        store.selectionClickWritten(first, revision: 1)
        store.applySelectionRevision(1)
        #expect(store.shownSelectedKeys.isEmpty)

        store.selectionClickFailed(second)
        #expect(store.shownSelectedKeys == ["/music/Album"])
    }
}
