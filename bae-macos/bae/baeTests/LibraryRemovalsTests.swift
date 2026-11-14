import BaeKit
import Testing

@testable import bae

/// What a closed library's removal asks the bridge for, in order.
private actor ClosedLibraryBridge {
    private(set) var calls: [String] = []

    func unsent(_ libraryId: String) -> Bool {
        calls.append("unsent \(libraryId)")
        return true
    }

    func remove(_ libraryId: String) -> BridgeLibraryRemoval {
        calls.append("remove \(libraryId)")
        return .removed
    }

    func removeUnopenable(_ libraryId: String) -> BridgeLibraryRemoval {
        calls.append("remove unopenable \(libraryId)")
        return .removed
    }

    func finishDeletion(_ libraryId: String) -> BridgeLibraryRemoval {
        calls.append("finish removing \(libraryId)")
        return .removed
    }
}

/// The removals a bridge call reads while it runs, set once they exist.
/// Held weakly: the removals hold that call, so a strong hold would keep
/// both, and the test's host with them, past the test.
@MainActor
private final class WatchedRemovals {
    weak var removals: LibraryRemovals?
}

private func library(_ state: BridgeLibraryState) -> BridgeLibrary {
    BridgeLibrary(
        id: "library-1",
        name: "Library",
        path: "/libraries/library-1",
        state: state,
        isActive: false,
        hasOtherLibraries: false,
        nextOpenableId: nil,
        previousOpenableId: nil
    )
}

private let deletionStopped = BridgeLibraryState.deletionUnfinished(
    lineKey: "core.error.deletion_unfinished"
)
private let openable = library(.openable(storage: nil))
private let unopenable: [BridgeLibrary] = [
    library(
        .unreadable(
            error: .Diagnostic(category: .config, detail: "settings")
        )
    ),
    library(
        .replacementUnfinished(
            by: .restore,
            lineKey: "core.error.replacement_unfinished.restore"
        )
    ),
    library(deletionStopped),
]

@MainActor
@Suite("LibraryRemovals closed libraries")
struct LibraryRemovalsClosedLibraryTests {
    private let host = TestHost.make()

    private let bridge = ClosedLibraryBridge()

    private var removals: LibraryRemovals {
        let bridge = bridge
        let host = host
        return LibraryRemovals(
            makeOpenProgress: { LibraryOpenProgress(host: host) },
            makeRemovalSteps: { LibraryClosingSteps(host: host) },
            makeFinishingSteps: { LibrarySetupSteps(host: host) },
            unsentOfClosed: { libraryId, _ in await bridge.unsent(libraryId) },
            removeClosed: { libraryId, _, _ in await bridge.remove(libraryId) },
            removeUnopenable: { libraryId, _ in
                await bridge.removeUnopenable(libraryId)
            },
            finishDeletion: { libraryId, _ in
                await bridge.finishDeletion(libraryId)
            },
            findUnfinished: { nil },
            finishUnfinished: { _ in }
        )
    }

    private func remove(
        with removals: LibraryRemovals,
        losing loss: RemovalLoss
    ) async {
        await withCheckedContinuation { finished in
            removals.remove(closedLibrary: openable, losing: loss) {
                finished.resume()
            }
        }
    }

    @Test("what removal loses is read without removing anything")
    func readingLosesNothing() async {
        let removals = removals
        let loss = await removals.loss(closedLibrary: openable)

        guard case .counted(let holdsUnsent) = loss else {
            Issue.record("a library that opens has its changes counted")
            return
        }
        #expect(holdsUnsent)
        #expect(await bridge.calls == ["unsent library-1"])
        #expect(removals.failed == nil)
    }

    @Test(
        "a library that can't open isn't opened to count",
        arguments: unopenable
    )
    func unopenableIsUncounted(library: BridgeLibrary) async {
        let removals = removals
        let loss = await removals.loss(closedLibrary: library)

        guard case .uncounted = loss else {
            Issue.record("a library that can't open has nothing counted")
            return
        }
        #expect(await bridge.calls.isEmpty)
        #expect(removals.failed == nil)
    }

    @Test("a confirmed count removes by opening the library, once")
    func confirmedRemovalRemoves() async {
        let removals = removals
        await remove(
            with: removals,
            losing: .counted(holdsUnsent: true)
        )

        #expect(await bridge.calls == ["remove library-1"])
        #expect(removals.failed == nil)
        #expect(removals.unfinishedError == nil)
        #expect(removals.removing == nil)
    }

    @Test("finishing a stopped removal finishes it without opening, once")
    func finishingRemovesWithoutOpening() async {
        let removals = removals
        await withCheckedContinuation { finished in
            removals.finishRemoving(library: library(deletionStopped)) {
                finished.resume()
            }
        }

        #expect(await bridge.calls == ["finish removing library-1"])
        #expect(removals.failed == nil)
        #expect(removals.unfinishedError == nil)
        #expect(removals.removing == nil)
    }

    @Test("a confirmed uncounted loss removes without opening, once")
    func confirmedUncountedRemovesWithoutOpening() async {
        let removals = removals
        await remove(with: removals, losing: .uncounted)

        #expect(await bridge.calls == ["remove unopenable library-1"])
        #expect(removals.failed == nil)
        #expect(removals.unfinishedError == nil)
        #expect(removals.removing == nil)
    }

    @Test("a removal that fails keeps what was confirmed, for Retry")
    func failedRemovalKeepsWhatWasConfirmed() async {
        let removals = LibraryRemovals(
            makeOpenProgress: { LibraryOpenProgress(host: host) },
            makeRemovalSteps: { LibraryClosingSteps(host: host) },
            makeFinishingSteps: { LibrarySetupSteps(host: host) },
            unsentOfClosed: { _, _ in
                Issue.record("nothing is counted")
                return false
            },
            removeClosed: { _, _, _ in
                Issue.record("the library isn't opened")
                return .removed
            },
            removeUnopenable: { _, _ in
                throw BridgeError.Diagnostic(category: .config, detail: "x")
            },
            finishDeletion: { _, _ in
                Issue.record("no removal is finished")
                return .removed
            },
            findUnfinished: { nil },
            finishUnfinished: { _ in }
        )

        await withCheckedContinuation { finished in
            removals.remove(
                closedLibrary: library(deletionStopped),
                losing: .uncounted
            ) { finished.resume() }
        }

        #expect(removals.failed?.library.id == "library-1")
        guard case .remove(.uncounted) = removals.failed?.retry else {
            Issue.record("Retry runs the confirmed removal again")
            return
        }
        #expect(removals.unfinishedError == nil)
        #expect(removals.unfinished == nil)
        #expect(removals.removing == nil)

        removals.dismissFailure()
        #expect(removals.failed == nil)
    }

    @Test("a removal that leaves files behind is not a failed removal")
    func leftoverFilesAreNotAFailure() async {
        let removals = LibraryRemovals(
            makeOpenProgress: { LibraryOpenProgress(host: host) },
            makeRemovalSteps: { LibraryClosingSteps(host: host) },
            makeFinishingSteps: { LibrarySetupSteps(host: host) },
            unsentOfClosed: { _, _ in
                Issue.record("nothing is counted")
                return false
            },
            removeClosed: { _, _, _ in
                .unfinished(
                    removal: BridgeUnfinishedRemoval(
                        error: .Diagnostic(category: .config, detail: "x"),
                        filesRemain: true
                    )
                )
            },
            removeUnopenable: { _, _ in
                Issue.record("the library opens")
                return .removed
            },
            finishDeletion: { _, _ in
                Issue.record("no removal is finished")
                return .removed
            },
            findUnfinished: { nil },
            finishUnfinished: { _ in }
        )

        await remove(
            with: removals,
            losing: .counted(holdsUnsent: false)
        )

        #expect(removals.failed == nil)
        #expect(removals.unfinished != nil)
        #expect(removals.unfinishedError != nil)
    }

    /// A library that can't open is removed with steps all the same, which
    /// its row draws in place of its detail line.
    @Test("a removal without opening writes its steps where the row reads them")
    func uncountedRemovalWritesItsSteps() async {
        let watched = WatchedRemovals()
        let removals = LibraryRemovals(
            makeOpenProgress: { LibraryOpenProgress(host: host) },
            makeRemovalSteps: { LibraryClosingSteps(host: host) },
            makeFinishingSteps: { LibrarySetupSteps(host: host) },
            unsentOfClosed: { _, _ in
                Issue.record("nothing is counted")
                return false
            },
            removeClosed: { _, _, _ in
                Issue.record("the library isn't opened")
                return .removed
            },
            removeUnopenable: { libraryId, closing in
                let shown = await MainActor.run { watched.removals?.removing }
                #expect(shown?.libraryId == libraryId)
                #expect(shown?.steps.closing === closing)
                return .removed
            },
            finishDeletion: { _, _ in
                Issue.record("no removal is finished")
                return .removed
            },
            findUnfinished: { nil },
            finishUnfinished: { _ in }
        )
        watched.removals = removals

        await remove(with: removals, losing: .uncounted)

        #expect(removals.failed == nil)
        #expect(removals.unfinishedError == nil)
        #expect(removals.removing == nil)
    }

    @Test("a count that fails asks again on Retry")
    func failedCountAsksAgain() async {
        let removals = LibraryRemovals(
            makeOpenProgress: { LibraryOpenProgress(host: host) },
            makeRemovalSteps: { LibraryClosingSteps(host: host) },
            makeFinishingSteps: { LibrarySetupSteps(host: host) },
            unsentOfClosed: { _, _ in
                throw BridgeError.Diagnostic(category: .database, detail: "x")
            },
            removeClosed: { _, _, _ in
                Issue.record("nothing is removed")
                return .removed
            },
            removeUnopenable: { _, _ in
                Issue.record("nothing is removed")
                return .removed
            },
            finishDeletion: { _, _ in
                Issue.record("no removal is finished")
                return .removed
            },
            findUnfinished: { nil },
            finishUnfinished: { _ in }
        )

        #expect(await removals.loss(closedLibrary: openable) == nil)
        guard case .ask = removals.failed?.retry else {
            Issue.record("Retry counts and asks again")
            return
        }
        #expect(removals.failed?.library.id == "library-1")
        #expect(removals.unfinishedError == nil)
    }
}

@Suite("LibraryRemovalConfirmation")
struct LibraryRemovalConfirmationTests {
    @Test("a library that syncs is removed, one that doesn't is deleted")
    func ending() {
        let synced = library(
            .openable(
                storage: .s3(
                    bucket: "music-library",
                    region: "us-east-1",
                    endpoint: nil,
                    profile: nil
                )
            )
        )
        #expect(LibraryRemovalConfirmation.Ending(of: synced) == .remove)
        #expect(LibraryRemovalConfirmation.Ending(of: openable) == .delete)
    }

    @Test(
        "a library that can't open is removed, never deleted",
        arguments: unopenable
    )
    func unopenableEnding(library: BridgeLibrary) {
        #expect(LibraryRemovalConfirmation.Ending(of: library) == .remove)
    }

    @Test("only unsent work the bridge counts warns")
    func warnsOfUnsent() {
        #expect(
            LibraryRemovalConfirmation.warnsOfUnsent(
                .counted(holdsUnsent: true)
            )
        )
        #expect(
            !LibraryRemovalConfirmation.warnsOfUnsent(
                .counted(holdsUnsent: false)
            )
        )
        #expect(!LibraryRemovalConfirmation.warnsOfUnsent(.uncounted))
    }

    @Test("a counted loss is followed by the message for the ending")
    func countedLoss() {
        let text = LibraryRemovalConfirmation.message(
            for: openable,
            losing: .counted(holdsUnsent: true)
        )
        #expect(text == LibraryRemovalConfirmation.message(syncs: false))
    }

    @Test(
        "a library that can't open says so, with no counts",
        arguments: unopenable
    )
    func uncountedLoss(library: BridgeLibrary) {
        let text = LibraryRemovalConfirmation.message(
            for: library,
            losing: .uncounted
        )
        #expect(
            text
                == String(
                    localized:
                        "This library cannot be opened, so any changes on this Mac that were not sent are lost. Its copy on this Mac is deleted; if the library is kept in the cloud, it stays there."
                )
        )
    }
}

/// A removal an earlier run left unfinished, as the host reports it until
/// finishing it succeeds.
private actor UnfinishedRemovalBridge {
    private var remaining = true
    private var failures: Int
    private(set) var finishes = 0
    /// The setup each finish was handed, to write its steps into.
    private(set) var setups: [BridgeLibrarySetup] = []

    init(failures: Int) {
        self.failures = failures
    }

    func find() -> BridgeUnfinishedRemoval? {
        remaining
            ? BridgeUnfinishedRemoval(
                error: .Diagnostic(category: .config, detail: "files remain"),
                filesRemain: true
            )
            : nil
    }

    func finish(_ setup: BridgeLibrarySetup) throws {
        finishes += 1
        setups.append(setup)
        if failures > 0 {
            failures -= 1
            throw BridgeError.Diagnostic(category: .internal, detail: "denied")
        }
        remaining = false
    }
}

@MainActor
@Suite("LibraryRemovals unfinished removals")
struct LibraryRemovalsUnfinishedTests {
    private let host = TestHost.make()

    @Test("a removal left unfinished shows at launch until a retry finishes it")
    func retryFinishesAnEarlierRemoval() async throws {
        let bridge = UnfinishedRemovalBridge(failures: 1)
        let removals = LibraryRemovals(
            makeOpenProgress: { LibraryOpenProgress(host: host) },
            makeRemovalSteps: { LibraryClosingSteps(host: host) },
            makeFinishingSteps: { LibrarySetupSteps(host: host) },
            unsentOfClosed: { _, _ in
                false
            },
            removeClosed: { _, _, _ in .removed },
            removeUnopenable: { _, _ in .removed },
            finishDeletion: { _, _ in
                Issue.record("no removal is finished")
                return .removed
            },
            findUnfinished: { await bridge.find() },
            finishUnfinished: { try await bridge.finish($0) }
        )

        await removals.loadUnfinished()
        #expect(removals.unfinished != nil)
        #expect(removals.unfinishedError != nil)

        removals.retryUnfinished()
        try await Wait.until { !removals.isRetrying }
        #expect(removals.unfinished != nil)
        #expect(removals.unfinishedError != nil)

        removals.retryUnfinished()
        let shown = removals.finishing
        #expect(shown != nil, "a retry shows its steps while it runs")
        try await Wait.until { !removals.isRetrying }
        #expect(removals.unfinished == nil)
        #expect(removals.unfinishedError == nil)
        #expect(removals.finishing == nil)
        #expect(await bridge.finishes == 2)
        #expect(
            await bridge.setups.last === shown?.setup,
            "the retry writes its steps where they show"
        )
    }

    @Test("a removal that stopped before the delete shows without Retry")
    func stoppedBeforeTheDelete() async {
        let removals = LibraryRemovals(
            makeOpenProgress: { LibraryOpenProgress(host: host) },
            makeRemovalSteps: { LibraryClosingSteps(host: host) },
            makeFinishingSteps: { LibrarySetupSteps(host: host) },
            unsentOfClosed: { _, _ in
                false
            },
            removeClosed: { _, _, _ in .removed },
            removeUnopenable: { _, _ in .removed },
            finishDeletion: { _, _ in
                Issue.record("no removal is finished")
                return .removed
            },
            findUnfinished: {
                BridgeUnfinishedRemoval(
                    error: .Diagnostic(category: .config, detail: "still here"),
                    filesRemain: false
                )
            },
            finishUnfinished: { _ in
                Issue.record("nothing to finish")
            }
        )

        await removals.loadUnfinished()
        #expect(removals.unfinished == nil)
        #expect(removals.unfinishedError != nil)
    }

    @Test("nothing shows when no removal was left unfinished")
    func nothingLeft() async {
        let removals = LibraryRemovals(
            makeOpenProgress: { LibraryOpenProgress(host: host) },
            makeRemovalSteps: { LibraryClosingSteps(host: host) },
            makeFinishingSteps: { LibrarySetupSteps(host: host) },
            unsentOfClosed: { _, _ in
                false
            },
            removeClosed: { _, _, _ in .removed },
            removeUnopenable: { _, _ in .removed },
            finishDeletion: { _, _ in
                Issue.record("no removal is finished")
                return .removed
            },
            findUnfinished: { nil },
            finishUnfinished: { _ in
                Issue.record("nothing to finish")
            }
        )

        await removals.loadUnfinished()
        #expect(removals.unfinished == nil)
        #expect(removals.unfinishedError == nil)
    }
}
