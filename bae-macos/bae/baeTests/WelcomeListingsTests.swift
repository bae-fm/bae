import BaeKit
import Testing

@testable import bae

/// What the welcome reads and removes through the bridge, in order.
private actor WelcomeBridge {
    private(set) var calls: [String] = []

    func listLibraries() -> [BridgeLibrary] {
        calls.append("list libraries")
        return []
    }

    func savedRestoreCodes() -> [BridgeSavedRestoreCode] {
        calls.append("list saved codes")
        return []
    }

    func remove(_ libraryId: String) -> BridgeLibraryRemoval {
        calls.append("remove \(libraryId)")
        return .removed
    }
}

private enum WelcomeTestError: Error {
    case notUsed
}

@MainActor
@Suite("WelcomeListings")
struct WelcomeListingsTests {
    private let bridge = WelcomeBridge()

    private var setup: LibrarySetup {
        let bridge = bridge
        return LibrarySetup(
            listLibraries: { await bridge.listLibraries() },
            createLibrary: { throw WelcomeTestError.notUsed },
            savedRestoreCodes: { await bridge.savedRestoreCodes() },
            makeBootstrap: {
                preconditionFailure("the welcome's lists start no restore")
            }
        )
    }

    private var removals: LibraryRemovals {
        let bridge = bridge
        return LibraryRemovals(
            unsentOfClosed: { _ in throw WelcomeTestError.notUsed },
            removeClosed: { await bridge.remove($0) },
            removeUnopenable: { _ in throw WelcomeTestError.notUsed },
            findUnfinished: { nil },
            finishUnfinished: {}
        )
    }

    /// A library removed from this Mac keeps its restore code saved, and core
    /// lists the code only for libraries not on this Mac, so the saved codes
    /// are read again with the libraries.
    @Test("removing a library reads both lists again")
    func removalReadsBothListsAgain() async {
        let listings = WelcomeListings()
        await listings.load(from: setup)
        let loaded = await bridge.calls
        #expect(Set(loaded) == ["list libraries", "list saved codes"])

        await listings.remove(
            closedLibrary: "library-1",
            losing: .unsent(BridgeUnsentWork(writes: 0, files: 0)),
            with: removals,
            from: setup
        )

        let calls = await bridge.calls
        #expect(calls.count == 5)
        #expect(calls[2] == "remove library-1")
        #expect(
            Set(calls[3...]) == ["list libraries", "list saved codes"]
        )
        #expect(listings.localLibraries.value?.isEmpty == true)
        #expect(listings.savedCodes.value?.isEmpty == true)
    }
}
