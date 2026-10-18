import BaeKit
import Testing

@testable import bae

/// The removals a closed library's removal asks the bridge for, answering a
/// refused removal as this device's unsent changes would.
private actor UnsentChangesBridge {
    private(set) var requests: [BridgeIfUnsent] = []

    func remove(_ unsent: BridgeIfUnsent) -> BridgeLibraryRemoval {
        requests.append(unsent)
        switch unsent {
        case .refuse: return .unsent(writes: 3, files: 1)
        case .discard: return .removed
        }
    }
}

@MainActor
@Suite("LibraryRemovals unsent changes")
struct LibraryRemovalsUnsentChangesTests {
    /// Let the removal's task deliver the refusal to the main actor.
    private func question(_ removals: LibraryRemovals) async -> UnsentRemoval? {
        for _ in 0..<1_000 where removals.unsent == nil {
            await Task.yield()
        }
        return removals.unsent
    }

    @Test("a refused removal asks, and removing anyway discards the changes")
    func removingAnywayDiscards() async {
        let bridge = UnsentChangesBridge()
        let removals = LibraryRemovals(
            removeClosed: { _, unsent in await bridge.remove(unsent) },
            findUnfinished: { nil },
            finishUnfinished: {}
        )

        await withCheckedContinuation { finished in
            removals.remove(closedLibrary: "library-1") {
                finished.resume()
            }
            Task {
                let asked = await question(removals)
                #expect(asked?.edits == UnsentEdits(writes: 3, files: 1))
                removals.discardUnsent()
            }
        }

        #expect(await bridge.requests == [.refuse, .discard])
        #expect(removals.unsent == nil)
        #expect(removals.error == nil)
    }

    @Test("keeping the library removes nothing more")
    func keepingRemovesNothing() async {
        let bridge = UnsentChangesBridge()
        let removals = LibraryRemovals(
            removeClosed: { _, unsent in await bridge.remove(unsent) },
            findUnfinished: { nil },
            finishUnfinished: {}
        )

        removals.remove(closedLibrary: "library-1") {
            Issue.record("a kept library's list doesn't change")
        }
        #expect(await question(removals) != nil)
        removals.keepUnsent()

        #expect(await bridge.requests == [.refuse])
        #expect(removals.unsent == nil)
        #expect(removals.removingLibraryId == nil)
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
            removeClosed: { _, _ in .removed },
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
            removeClosed: { _, _ in .removed },
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
            removeClosed: { _, _ in .removed },
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
