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
        keysBetween:
            @escaping @Sendable (
                BridgeImportListRequest,
                Data,
                Data
            ) async throws -> [Data] = { _, _, _ in [] },
        allKeys:
            @escaping @Sendable (BridgeImportListRequest) async throws -> [Data] =
            { _ in [] },
        combine:
            @escaping @Sendable (BridgeImportListRequest, [Data]) async throws
            -> Data = { _, _ in
                throw StubError.notImplemented
            },
        separate:
            @escaping @Sendable (BridgeImportListRequest, [Data]) async throws
            -> [Data] = { _, _ in
                throw StubError.notImplemented
            },
        progressStream:
            @escaping @Sendable () -> LatestStream<
                BridgeSelectionActionProgress
            > = { .bulkActionProgress() },
        run:
            @escaping @Sendable (
                BridgeImportListRequest,
                BridgeCandidateAction,
                [Data],
                LatestStream<BridgeSelectionActionProgress>
            ) async throws -> [BridgeSelectionActionFailure] = { _, _, _, _ in
                []
            }
    ) -> ImportSelectionOperations {
        ImportSelectionOperations(
            keysBetween: keysBetween,
            allKeys: allKeys,
            sourceFolders: inert.sourceFolders,
            combine: combine,
            separate: separate,
            progressStream: progressStream,
            run: run
        )
    }
}

extension ImportSelection {
    /// A selection over `operations` on a list showing Found, as the sidebar
    /// shows it once it has read its first list.
    static func onList(
        operations: ImportSelectionOperations = .inert
    ) -> ImportSelection {
        let selection = ImportSelection(operations: operations)
        selection.show(
            BridgeImportListRequest(
                view: BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    pendingFilter: .all,
                    order: .newestFirst
                ),
                folds: .allOpen
            )
        )
        return selection
    }
}

@MainActor
@Suite(.timeLimit(.minutes(1)))
struct ImportSelectionTests {
    @Test("Acting on the selection before the sidebar shows a list fails")
    func actingBeforeAListFails() async {
        let selection = ImportSelection(
            operations: .stub(run: { _, _, _, _ in
                Issue.record("no list to act on")
                return []
            })
        )
        var errors: [any Error] = []
        selection.onError = { errors.append($0) }
        selection.selectAll()
        #expect(errors.count == 1)
        await #expect(throws: ImportSelectionError.self) {
            try await selection.sourceFolders()
        }
        let uiStore = UiStore()
        await selection.perform(.skip, uiStore: uiStore)
        #expect(uiStore.lastError != nil)
    }

    @Test("A running action can be cancelled and rejects a second one")
    func cancelRunningAction() async throws {
        let entered = AsyncStream<Void>.makeStream()
        let selection = ImportSelection.onList(
            operations: .stub(run: { _, _, _, _ in
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
        "The action runs in core over the kept keys, and the error line names each row it failed on"
    )
    func failuresNameEachRow() async {
        let requested = CallLog<BridgeCandidateAction>()
        let sent = CallLog<[Data]>()
        let album = Data("/music/Album".utf8)
        let other = Data("/music/Other Album".utf8)
        let selection = ImportSelection.onList(
            operations: .stub(run: { _, action, keys, _ in
                requested.record(action)
                sent.record(keys)
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
        selection.select([album, other])
        let uiStore = UiStore()
        await selection.perform(.import, uiStore: uiStore)

        #expect(requested.all == [.import])
        #expect(sent.all.map(Set.init) == [[album, other]])
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
        let selection = ImportSelection.onList(
            operations: .stub(
                progressStream: { progress.stream },
                run: { _, _, _, _ in
                    progress.set(
                        BridgeSelectionActionProgress(
                            completed: 1,
                            total: 3,
                            fraction: 1.0 / 3
                        )
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
            shown.last
                == BridgeSelectionActionProgress(
                    completed: 1,
                    total: 3,
                    fraction: 1.0 / 3
                )
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
        let selection = PreviewData.selectedImportTabPair()
        let counts = Dictionary(
            uniqueKeysWithValues:
                ImportCandidateActionOffer.selection(
                    selection.summary
                )
                .map { ($0.action, $0.applicable) }
        )
        #expect(counts[.import] == 1)
        #expect(counts[.skip] == 2)
        #expect(counts[.restore] == nil)
        #expect(counts[.combine] == 0)
        #expect(counts[.revealFolder] == 2)
    }
}

/// Combining and separating send the kept keys, and the release they make
/// or return becomes the selection.
@MainActor
@Suite("The selection after combining and separating")
struct ImportSelectionReplacementTests {
    nonisolated private static let parts = [
        Data("/music/Box/1".utf8), Data("/music/Box/2".utf8),
    ]
    nonisolated private static let combined = Data("grouping-new".utf8)

    @Test("combining selects the release it makes")
    func combiningSelectsTheRelease() async throws {
        let sent = CallLog<[Data]>()
        let selection = ImportSelection.onList(
            operations: .stub(combine: { _, keys in
                sent.record(keys)
                return Self.combined
            })
        )
        selection.select(Set(Self.parts))
        let key = try await selection.combine()
        #expect(key == Self.combined)
        #expect(sent.all.map(Set.init) == [Set(Self.parts)])
        #expect(selection.keys == [Self.combined])
    }

    @Test("separating selects the releases it returns")
    func separatingSelectsTheReleases() async throws {
        let selection = ImportSelection.onList(
            operations: .stub(separate: { _, _ in Self.parts })
        )
        selection.select([Self.combined])
        try await selection.separate()
        #expect(selection.keys == Set(Self.parts))
    }
}

/// A click while core reads the keys of a Shift-click range or of Select
/// All cancels that read, so its answer never replaces the click.
@MainActor
@Suite("A later click cancels a key read", .timeLimit(.minutes(1)))
struct ImportSelectionKeysReadTests {
    nonisolated private static let clicked = Data("/music/Clicked".utf8)

    /// Operations whose key reads wait until cancelled, recording each read
    /// that starts in `entered` and each one cancelled in `cancelled`.
    private static func waiting(
        entered: AsyncStream<Void>.Continuation,
        cancelled: CallLog<Void>
    ) -> ImportSelectionOperations {
        let read: @Sendable () async throws -> [Data] = {
            entered.yield(())
            do { try await Task.neverAnswers() }
            catch {
                cancelled.record(())
                throw error
            }
        }
        return .stub(
            keysBetween: { _, _, _ in try await read() },
            allKeys: { _ in try await read() }
        )
    }

    @Test("a click while core reads Select All's keys cancels the read")
    func clickCancelsSelectAll() async throws {
        let entered = AsyncStream<Void>.makeStream()
        let cancelled = CallLog<Void>()
        let selection = ImportSelection.onList(
            operations: Self.waiting(
                entered: entered.continuation,
                cancelled: cancelled
            )
        )
        selection.selectAll()
        var iterator = entered.stream.makeAsyncIterator()
        await iterator.next()
        selection.select([Self.clicked])
        try await Wait.until { cancelled.all.count == 1 }
        #expect(selection.keys == [Self.clicked])
    }

    @Test("a click while core reads a Shift-click's range cancels the read")
    func clickCancelsRange() async throws {
        let entered = AsyncStream<Void>.makeStream()
        let cancelled = CallLog<Void>()
        let selection = ImportSelection.onList(
            operations: Self.waiting(
                entered: entered.continuation,
                cancelled: cancelled
            )
        )
        let (from, to) = (Data("/music/A".utf8), Data("/music/C".utf8))
        selection.edit(
            .adjust(
                add: [from, to],
                remove: [],
                range: .init(from: from, to: to)
            )
        )
        #expect(selection.keys == [from, to])
        var iterator = entered.stream.makeAsyncIterator()
        await iterator.next()
        selection.select([Self.clicked])
        try await Wait.until { cancelled.all.count == 1 }
        #expect(selection.keys == [Self.clicked])
    }

    @Test("the range core reads joins the kept keys")
    func rangeJoinsTheKeptKeys() async throws {
        let between = [Data("/music/A".utf8), Data("/music/B".utf8)]
        let selection = ImportSelection.onList(
            operations: .stub(keysBetween: { _, _, _ in between })
        )
        selection.select([Self.clicked])
        selection.edit(
            .adjust(
                add: [between[0]],
                remove: [],
                range: .init(from: between[0], to: between[1])
            )
        )
        try await Wait.until { selection.keys.count == 3 }
        #expect(selection.keys == Set(between + [Self.clicked]))
    }
}

/// ImportSelection makes each change a click or key asks for to the keys it
/// holds when the change arrives.
@MainActor
@Suite("Edits to the kept keys")
struct ImportSelectionEditTests {
    @Test("a Command-click changes its own row and keeps a hidden one")
    func adjustKeepsTheOtherKeys() {
        let (hidden, shown, clicked) = (
            Data("/music/Hidden".utf8), Data("/music/Shown".utf8),
            Data("/music/Clicked".utf8)
        )
        let selection = ImportSelection.onList()
        selection.select([hidden, shown])
        selection.edit(
            .adjust(add: [clicked], remove: [shown], range: nil)
        )
        #expect(selection.keys == [hidden, clicked])
        selection.edit(.only([shown]))
        #expect(selection.keys == [shown])
    }
}
