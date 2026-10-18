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
        let removals = LibraryRemovals { _, unsent in
            await bridge.remove(unsent)
        }

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
        let removals = LibraryRemovals { _, unsent in
            await bridge.remove(unsent)
        }

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
