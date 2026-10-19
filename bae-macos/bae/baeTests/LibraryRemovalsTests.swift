import BaeKit
import Testing

@testable import bae

/// What a closed library's removal asks the bridge for, in order.
private actor ClosedLibraryBridge {
    private(set) var calls: [String] = []

    func unsent(_ libraryId: String) -> BridgeUnsentWork {
        calls.append("unsent \(libraryId)")
        return BridgeUnsentWork(writes: 3, files: 1)
    }

    func remove(_ libraryId: String) -> BridgeLibraryRemoval {
        calls.append("remove \(libraryId)")
        return .removed
    }

    func removeUnopenable(_ libraryId: String) -> BridgeLibraryRemoval {
        calls.append("remove unopenable \(libraryId)")
        return .removed
    }
}

private func library(_ state: BridgeLibraryState) -> BridgeLibrary {
    BridgeLibrary(
        id: "library-1",
        name: "Library",
        path: "/libraries/library-1",
        state: state,
        isActive: false
    )
}

private let openable = library(.openable(storage: nil))
private let unopenable: [BridgeLibrary] = [
    library(
        .unreadable(
            error: .Diagnostic(category: .config, detail: "settings")
        )
    ),
    library(.replacementUnfinished(by: .restore)),
]

@MainActor
@Suite("LibraryRemovals closed libraries")
struct LibraryRemovalsClosedLibraryTests {
    private let bridge = ClosedLibraryBridge()

    private var removals: LibraryRemovals {
        let bridge = bridge
        return LibraryRemovals(
            unsentOfClosed: { await bridge.unsent($0) },
            removeClosed: { await bridge.remove($0) },
            removeUnopenable: { await bridge.removeUnopenable($0) },
            findUnfinished: { nil },
            finishUnfinished: {}
        )
    }

    private func remove(
        with removals: LibraryRemovals,
        losing loss: RemovalLoss
    ) async {
        await withCheckedContinuation { finished in
            removals.remove(closedLibrary: "library-1", losing: loss) {
                finished.resume()
            }
        }
    }

    @Test("what removal loses is read without removing anything")
    func readingLosesNothing() async {
        let removals = removals
        let loss = await removals.loss(closedLibrary: openable)

        guard case .unsent(let unsent) = loss else {
            Issue.record("a library that opens has its changes counted")
            return
        }
        #expect(unsent.writes == 3)
        #expect(unsent.files == 1)
        #expect(await bridge.calls == ["unsent library-1"])
        #expect(removals.error == nil)
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
        #expect(removals.error == nil)
    }

    @Test("a confirmed count removes by opening the library, once")
    func confirmedRemovalRemoves() async {
        let removals = removals
        await remove(
            with: removals,
            losing: .unsent(BridgeUnsentWork(writes: 3, files: 1))
        )

        #expect(await bridge.calls == ["remove library-1"])
        #expect(removals.error == nil)
        #expect(removals.removingLibraryId == nil)
    }

    @Test("a confirmed uncounted loss removes without opening, once")
    func confirmedUncountedRemovesWithoutOpening() async {
        let removals = removals
        await remove(with: removals, losing: .uncounted)

        #expect(await bridge.calls == ["remove unopenable library-1"])
        #expect(removals.error == nil)
        #expect(removals.removingLibraryId == nil)
    }

    @Test("a refused removal of a library that can't open shows why")
    func refusedUnopenableRemovalShowsError() async {
        let removals = LibraryRemovals(
            unsentOfClosed: { _ in
                Issue.record("nothing is counted")
                return BridgeUnsentWork(writes: 0, files: 0)
            },
            removeClosed: { _ in
                Issue.record("the library isn't opened")
                return .removed
            },
            removeUnopenable: { _ in
                throw BridgeError.Diagnostic(category: .config, detail: "x")
            },
            findUnfinished: { nil },
            finishUnfinished: {}
        )

        await remove(with: removals, losing: .uncounted)

        #expect(removals.error != nil)
        #expect(removals.unfinished == nil)
        #expect(removals.removingLibraryId == nil)
    }

    @Test("a count that can't be read shows why and asks nothing")
    func unreadableCountShowsError() async {
        let removals = LibraryRemovals(
            unsentOfClosed: { _ in
                throw BridgeError.Diagnostic(category: .database, detail: "x")
            },
            removeClosed: { _ in
                Issue.record("nothing is removed")
                return .removed
            },
            removeUnopenable: { _ in
                Issue.record("nothing is removed")
                return .removed
            },
            findUnfinished: { nil },
            finishUnfinished: {}
        )

        #expect(await removals.loss(closedLibrary: openable) == nil)
        #expect(removals.error != nil)
    }
}

@Suite("LibraryRemovalConfirmation")
struct LibraryRemovalConfirmationTests {
    @Test("a counted loss lists the changes, then the consequence")
    func countedLoss() {
        let text = LibraryRemovalConfirmation.text(
            for: openable,
            losing: .unsent(BridgeUnsentWork(writes: 2, files: 0))
        )
        #expect(
            text.hasSuffix(
                LibraryRemovalConfirmation.consequence + " "
                    + LibraryRemovalConfirmation.message(hasCloudHome: false)
            )
        )
    }

    @Test(
        "a library that can't open says so, with no counts",
        arguments: unopenable
    )
    func uncountedLoss(library: BridgeLibrary) {
        let text = LibraryRemovalConfirmation.text(
            for: library,
            losing: .uncounted
        )
        #expect(
            text
                == String(
                    localized:
                        "This library can't be opened, so any changes on this Mac that weren't sent are lost. Its copy on this Mac is deleted; any copy in the cloud is untouched."
                )
        )
    }
}

@Suite("BridgeUnsentWork confirmation")
struct BridgeUnsentWorkConfirmationTests {
    @Test("nothing waiting leaves only the message")
    func nothingWaiting() {
        let text = BridgeUnsentWork(writes: 0, files: 0)
            .confirmation(consequence: "Lost.", message: "Message.")
        #expect(text == "Message.")
    }

    @Test("waiting changes come first, then the consequence")
    func changesWaiting() {
        let text = BridgeUnsentWork(writes: 2, files: 0)
            .confirmation(consequence: "Lost.", message: "Message.")
        #expect(text.hasSuffix("Lost. Message."))
        #expect(text != "Lost. Message.")
    }
}

/// A removal an earlier run left unfinished, as the host reports it until
/// finishing it succeeds.
private actor UnfinishedRemovalBridge {
    private var remaining = true
    private var failures: Int
    private(set) var finishes = 0

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

    func finish() throws {
        finishes += 1
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
    private func settled(_ removals: LibraryRemovals) async {
        for _ in 0..<1_000 where removals.isRetrying {
            await Task.yield()
        }
    }

    @Test("a removal left unfinished shows at launch until a retry finishes it")
    func retryFinishesAnEarlierRemoval() async {
        let bridge = UnfinishedRemovalBridge(failures: 1)
        let removals = LibraryRemovals(
            unsentOfClosed: { _ in BridgeUnsentWork(writes: 0, files: 0) },
            removeClosed: { _ in .removed },
            removeUnopenable: { _ in .removed },
            findUnfinished: { await bridge.find() },
            finishUnfinished: { try await bridge.finish() }
        )

        await removals.loadUnfinished()
        #expect(removals.unfinished != nil)
        #expect(removals.error != nil)

        removals.retryUnfinished()
        await settled(removals)
        #expect(removals.unfinished != nil)
        #expect(removals.error != nil)

        removals.retryUnfinished()
        await settled(removals)
        #expect(removals.unfinished == nil)
        #expect(removals.error == nil)
        #expect(await bridge.finishes == 2)
    }

    @Test("a removal that stopped before the delete shows without Retry")
    func stoppedBeforeTheDelete() async {
        let removals = LibraryRemovals(
            unsentOfClosed: { _ in BridgeUnsentWork(writes: 0, files: 0) },
            removeClosed: { _ in .removed },
            removeUnopenable: { _ in .removed },
            findUnfinished: {
                BridgeUnfinishedRemoval(
                    error: .Diagnostic(category: .config, detail: "still here"),
                    filesRemain: false
                )
            },
            finishUnfinished: {
                Issue.record("nothing to finish")
            }
        )

        await removals.loadUnfinished()
        #expect(removals.unfinished == nil)
        #expect(removals.error != nil)
    }

    @Test("nothing shows when no removal was left unfinished")
    func nothingLeft() async {
        let removals = LibraryRemovals(
            unsentOfClosed: { _ in BridgeUnsentWork(writes: 0, files: 0) },
            removeClosed: { _ in .removed },
            removeUnopenable: { _ in .removed },
            findUnfinished: { nil },
            finishUnfinished: {
                Issue.record("nothing to finish")
            }
        )

        await removals.loadUnfinished()
        #expect(removals.unfinished == nil)
        #expect(removals.error == nil)
    }
}
