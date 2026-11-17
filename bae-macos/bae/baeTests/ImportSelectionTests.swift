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
                BridgeTriageTab,
                BridgeImportListOrder,
                Data,
                Data
            ) async throws -> [Data] = { _, _, _, _ in [] },
        allKeys: @escaping @Sendable () async throws -> [Data] = { [] },
        combine: @escaping @Sendable ([Data]) async throws -> Data = { _ in
            throw StubError.notImplemented
        },
        separate: @escaping @Sendable ([Data]) async throws -> [Data] = { _ in
            throw StubError.notImplemented
        },
        progressStream:
            @escaping @Sendable () -> LatestStream<
                BridgeSelectionActionProgress
            > = { .bulkActionProgress() },
        run:
            @escaping @Sendable (
                BridgeCandidateAction,
                [Data],
                LatestStream<BridgeSelectionActionProgress>
            ) async throws -> [BridgeSelectionActionFailure] = { _, _, _ in [] }
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

@MainActor
@Suite(.timeLimit(.minutes(1)))
struct ImportSelectionTests {
    @Test("A running action can be cancelled and rejects a second one")
    func cancelRunningAction() async throws {
        let entered = AsyncStream<Void>.makeStream()
        let selection = ImportSelection(
            operations: .stub(run: { _, _, _ in
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
        "The action runs in core over the kept keys, and each row it failed on is named"
    )
    func failuresNameEachRow() async {
        let requested = CallLog<BridgeCandidateAction>()
        let sent = CallLog<[Data]>()
        let album = Data("/music/Album".utf8)
        let other = Data("/music/Other Album".utf8)
        let selection = ImportSelection(
            operations: .stub(run: { action, keys, _ in
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
        let selection = ImportSelection(
            operations: .stub(
                progressStream: { progress.stream },
                run: { _, _, _ in
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
        let selection = ImportSelection(
            operations: .stub(combine: {
                sent.record($0)
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
        let selection = ImportSelection(
            operations: .stub(separate: { _ in Self.parts })
        )
        selection.select([Self.combined])
        try await selection.separate()
        #expect(selection.keys == Set(Self.parts))
    }
}
