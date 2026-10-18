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
}

@MainActor
@Suite("LibraryRemovals closed libraries")
struct LibraryRemovalsClosedLibraryTests {
    @Test("what removal loses is read without removing anything")
    func readingLosesNothing() async {
        let bridge = ClosedLibraryBridge()
        let removals = LibraryRemovals(
            unsentOfClosed: { await bridge.unsent($0) },
            removeClosed: { await bridge.remove($0) },
            findUnfinished: { nil },
            finishUnfinished: {}
        )

        let unsent = await removals.unsentWork(closedLibrary: "library-1")

        #expect(unsent?.writes == 3)
        #expect(unsent?.files == 1)
        #expect(await bridge.calls == ["unsent library-1"])
        #expect(removals.error == nil)
    }

    @Test("a confirmed removal removes once")
    func confirmedRemovalRemoves() async {
        let bridge = ClosedLibraryBridge()
        let removals = LibraryRemovals(
            unsentOfClosed: { await bridge.unsent($0) },
            removeClosed: { await bridge.remove($0) },
            findUnfinished: { nil },
            finishUnfinished: {}
        )

        await withCheckedContinuation { finished in
            removals.remove(closedLibrary: "library-1") {
                finished.resume()
            }
        }

        #expect(await bridge.calls == ["remove library-1"])
        #expect(removals.error == nil)
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
            findUnfinished: { nil },
            finishUnfinished: {}
        )

        #expect(await removals.unsentWork(closedLibrary: "library-1") == nil)
        #expect(removals.error != nil)
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
