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
        lock.recording { values.append(value) }
    }
}

extension ImportSelectionOperations {
    /// The inert operations with the ones a test watches replaced.
    static func stub(
        change:
            @escaping @Sendable (BridgeSelectionChange) async throws -> Void =
            { _ in },
        selectAll: @escaping @Sendable () async throws -> Void = {},
        combine: @escaping @Sendable () async throws -> Data = {
            throw StubError.notImplemented
        },
        separate: @escaping @Sendable () async throws -> [Data] = {
            throw StubError.notImplemented
        },
        progressStream:
            @escaping @Sendable () -> LatestStream<
                BridgeSelectionActionProgress
            > = { .bulkActionProgress() },
        run:
            @escaping @Sendable (
                BridgeCandidateAction,
                LatestStream<BridgeSelectionActionProgress>
            ) async throws -> [BridgeSelectionActionFailure] = { _, _ in [] }
    ) -> ImportSelectionOperations {
        ImportSelectionOperations(
            change: change,
            selectAll: selectAll,
            sourceFolders: inert.sourceFolders,
            combine: combine,
            separate: separate,
            progressStream: progressStream,
            run: run
        )
    }
}

@MainActor
@Suite(.timeLimit(.minutes(1)))
struct ImportSelectionTests {
    @Test("A running action can be cancelled and rejects a second one")
    func cancelRunningAction() async throws {
        let entered = AsyncStream<Void>.makeStream()
        let selection = ImportSelection(
            operations: .stub(run: { _, _ in
                entered.continuation.yield(())
                try await Task.neverAnswers()
            })
        )
        let uiStore = UiStore()
        let started = selection.start(
            .skip,
            uiStore: uiStore,
            before: {}
        )
        let task = try #require(started)
        var iterator = entered.stream.makeAsyncIterator()
        await iterator.next()
        let second = selection.start(
            .skip,
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
        let selection = ImportSelection(
            operations: .stub(run: { action, _ in
                requested.record(action)
                return [
                    BridgeSelectionActionFailure(
                        candidateKey: Data("/music/Album".utf8),
                        name: "Album",
                        error: .Diagnostic(
                            category: .candidateBeingIdentified,
                            detail: "still being identified"
                        )
                    ),
                    BridgeSelectionActionFailure(
                        candidateKey: Data("/music/Other Album".utf8),
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
        await selection.perform(.import, uiStore: uiStore)

        #expect(requested.all == [.import])
        let line = uiStore.lastError?.line ?? ""
        #expect(line.contains("Album"))
        #expect(line.contains("Other Album"))
        #expect(
            line.contains(
                localizedCoreString(
                    "core.import.error.candidate_being_identified"
                )
            )
        )
        #expect(!selection.isRunning)
    }

    /// The pane's native line follows the stream the action's run feeds,
    /// and the action stops running once the run ends.
    @Test("Core's progress reaches the pane's line while the action runs")
    func progressFollowsCore() async throws {
        let release = AsyncStream<Void>.makeStream()
        let progress = LatestStream<BridgeSelectionActionProgress>.written()
        let selection = ImportSelection(
            operations: .stub(
                progressStream: { progress.stream },
                run: { _, _ in
                    progress.set(
                        BridgeSelectionActionProgress(completed: 1, total: 3)
                    )
                    for await _ in release.stream { break }
                    return []
                }
            )
        )
        let started = selection.start(
            .skip,
            uiStore: UiStore(),
            before: {}
        )
        let task = try #require(started)
        try await Wait.until { selection.running != nil }
        let running = try #require(selection.running)
        #expect(running.action == .skip)
        let view = NSView()
        var shown: [BridgeSelectionActionProgress?] = []
        let following = FrameFollowing<BridgeSelectionActionProgress>(
            view: view
        ) {
            shown.append($0)
            Wait.happened()
        }
        following.follow(running.progress)
        try await Wait.until {
            shown.last == BridgeSelectionActionProgress(completed: 1, total: 3)
        }
        following.stop()
        release.continuation.yield(())
        await task.value
        #expect(selection.running == nil)
        #expect(!selection.isRunning)
    }

    /// How the pane draws them is the `import-bulk-selection` shot's.
    @Test("The pane draws the selection's offers with their counts")
    func selectionPaneRenders() {
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
    }
}

/// What the list shows selected is what the rows it holds say: a click
/// writes the selection, and the windows holding the rows it changed deliver
/// them selected.
@MainActor
@Suite("The selection the list's rows show")
struct ListSelectionTests {
    private static let album = Data("/music/Album".utf8)
    private static let otherAlbum = Data("/music/Other Album".utf8)

    private static func list(
        selecting selected: Data?
    ) -> PaginatedList<BridgeImportListItem> {
        let items = [album, otherAlbum]
            .map { key in
                var row = PreviewData.triageRowIdentified
                row.candidateKey = key
                row.selected = key == selected
                return PreviewData.candidateItem(row)
            }
        let list = PaginatedList<BridgeImportListItem>(
            queries: .fixed(items, answers: UnansweredCount()),
            onError: { _ in }
        )
        list.preloadRows(rows: items)
        return list
    }

    @Test("the rows' own selected flags are the loaded selection")
    func theRowsSayWhatIsSelected() {
        #expect(Self.list(selecting: Self.album).selectedKeys == [Self.album])
        #expect(
            Self.list(selecting: Self.otherAlbum).selectedKeys == [
                Self.otherAlbum
            ]
        )
        #expect(Self.list(selecting: nil).selectedKeys.isEmpty)
    }
}
