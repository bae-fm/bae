import BaeKit
import Foundation
import Observation
import Testing
import XCTest

@testable import bae

private struct ReadFailed: Error {}

extension ImportListHandle {
    /// A handle over `items` whose view changes, folds and reads are the
    /// given stand-ins, and change nothing otherwise.
    static func stub(
        items: [BridgeImportListItem] = [],
        queries: BridgeListQueries<BridgeImportListItem>? = nil,
        setView:
            @escaping @Sendable (BridgeImportListView) async throws
            -> BridgeImportListOpening? = { _ in nil },
        setGroupFolded:
            @escaping @Sendable (BridgeFolderReleaseDecisionKey, Bool)
            async throws -> Void = { _, _ in },
        setAllGroupsFolded: @escaping @Sendable (Bool) async throws -> Void = {
            _ in
        },
        locateCandidate:
            @escaping @Sendable (Data) async throws
            -> BridgeImportCandidateListLocation? = { _ in nil },
        firstIdentifyingCandidate:
            @escaping @Sendable () async throws -> Data? = { nil }
    ) -> ImportListHandle {
        let fixed = ImportListHandle.fixed(items: items, tab: .pending)
        return ImportListHandle(
            queries: queries ?? fixed.queries,
            summary: fixed.summary,
            filterEntries: fixed.filterEntries,
            selection: fixed.selection,
            setView: setView,
            setGroupFolded: setGroupFolded,
            setAllGroupsFolded: setAllGroupsFolded,
            locateCandidate: locateCandidate,
            firstIdentifyingCandidate: firstIdentifyingCandidate
        )
    }

    /// A handle whose every window read fails.
    static func failing() -> ImportListHandle {
        stub(queries: .failing(ReadFailed(), answers: UnansweredCount()))
    }
}

private final class RecordedImportView: @unchecked Sendable {
    private let lock = NSLock()
    private var view: BridgeImportListView?

    var last: BridgeImportListView? {
        lock.withLock { view }
    }

    func set(_ view: BridgeImportListView) {
        lock.recording { self.view = view }
    }
}

/// A view change that waits until the test lets it answer.
private final class AppliedViewResolver: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, Never>?
    private var requested: [BridgeImportListView] = []

    var requests: [BridgeImportListView] {
        lock.withLock { requested }
    }

    func wait(for view: BridgeImportListView) async {
        await withCheckedContinuation { continuation in
            lock.recording {
                requested.append(view)
                self.continuation = continuation
            }
        }
    }

    func resolve() {
        let continuation = lock.withLock {
            let continuation = self.continuation
            self.continuation = nil
            return continuation
        }
        continuation?.resume()
    }
}

private func candidateItem(_ index: Int) -> BridgeImportListItem {
    let key = candidateKey(index)
    return .candidate(
        stableKey: candidateStableKey(key),
        row: BridgeTriageRow(
            candidateKey: key,
            folderName: "Release \(index)",
            watchedFolderPath: Data("/library".utf8),
            displayPath: "Release \(index)",
            actionable: true,
            placement: .skipped,
            live: BridgeCandidateLiveState(
                identifying: nil,
                readingPick: false,
                import: nil,
                actions: [],
                standing: .notLookedUp,
                badge: nil
            ),
            metadataSummary: nil,
            cover: nil,
            importStatus: nil,
            upload: nil,
            metadataProvenance: nil,
            reading: .unidentified,
            selected: false
        ),
        headerKey: nil
    )
}

private func candidateKey(_ index: Int) -> Data {
    Data("/library/release-\(index)".utf8)
}

/// A list key for the row of the folder `key` names, spelled the way core
/// spells one: the key's bytes in hex.
private func candidateStableKey(_ key: Data) -> String {
    "candidate:" + key.map { String(format: "%02x", $0) }.joined()
}

/// The import tab picks its pane before drawing one, so a failed first window
/// read has to reach it as `initialLoadError`.
@MainActor
@Suite("Import list slot read failures")
struct ImportListSlotTests {
    @Test("sort choices update the view and survive recreating the slot")
    func sortPreferenceReconfiguresAndPersists() async throws {
        let suite = "ImportListSlotTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let requests = RecordedImportView()
        func makeSlot() -> ImportListSlot {
            ImportListSlot(
                importStore: ImportStore(),
                uiStore: UiStore(),
                selection: ImportSelection(),
                defaults: defaults,
                makeHandle: { view in
                    requests.set(view)
                    return .stub(setView: {
                        requests.set($0)
                        return nil
                    })
                }
            )
        }
        let slot = makeSlot()
        #expect(slot.sortOrder == .newestFirst)
        slot.startLoad()
        try await Wait.until { slot.list != nil }
        #expect(requests.last?.order == .newestFirst)

        for order: BridgeImportListOrder in [
            .oldestFirst, .pathDescending, .pathAscending, .newestFirst,
        ] {
            slot.setSortOrder(order)
            #expect(slot.sortOrder == order)
            try await Wait.until { requests.last?.order == order }
            let reopened = makeSlot()
            #expect(reopened.sortOrder == order)
            reopened.startLoad()
            try await Wait.until { reopened.list != nil }
            #expect(requests.last?.order == order)
        }
    }

    @Test("a failed first window becomes the slot's failure and an alert")
    func aFailedFirstWindowIsSurfaced() async throws {
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: { _ in .failing() }
        )

        #expect(slot.loadFailure == nil)
        #expect(uiStore.lastError == nil)

        slot.startLoad()
        try await Wait.until { slot.loadFailure != nil }

        #expect(slot.loadFailure != nil)
        // The same failure is raised as the global alert.
        #expect(uiStore.lastError != nil)
    }

    /// The slot follows the candidate core says the count waits on.
    @Test("going to the first identifying candidate follows core's answer")
    func revealFirstIdentifyingFollowsCoresAnswer() async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.done)
        uiStore.setImportCandidateFilterText("hidden")
        let target = candidateKey(61)
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: { _ in
                .stub(
                    items: (0..<80).map(candidateItem),
                    locateCandidate: { key in
                        located(key, at: 61, tab: .pending)
                    },
                    firstIdentifyingCandidate: { target }
                )
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(0) != nil }

        let revealed = try await slot.revealFirstIdentifying()

        #expect(revealed?.candidateKey == target)
        #expect(revealed?.position == 61)
        #expect(uiStore.importCandidateTab == .pending)
        #expect(uiStore.importCandidateFilterText.isEmpty)
        #expect(slot.list?.idAt(61) == candidateStableKey(target))
    }

    @Test("nothing identifying reveals nothing")
    func revealFirstIdentifyingWithNothingIdentifying() async throws {
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: { _ in
                .stub(
                    items: (0..<80).map(candidateItem),
                    locateCandidate: { _ in
                        Issue.record("nothing to locate")
                        return nil
                    }
                )
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(0) != nil }

        #expect(try await slot.revealFirstIdentifying() == nil)
    }

    /// Each selection write waits for the ones before it, so core applies
    /// them in the order the person made them. A view change writes nothing
    /// to the selection: core's list keeps it to the rows the view shows.
    @Test("selection writes land in order, and a view change writes none")
    func selectionWritesLandInOrder() async throws {
        let writes = CallLog<String>()
        let toggleEntered = AsyncStream<Void>.makeStream()
        let releaseToggle = AsyncStream<Void>.makeStream()
        let selection = ImportSelection(
            operations: .stub(
                change: { _ in
                    toggleEntered.continuation.yield(())
                    for await _ in releaseToggle.stream { break }
                    writes.record("toggle")
                },
                selectAll: { writes.record("select all") }
            )
        )
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: selection,
            makeHandle: { _ in .stub(items: (0..<80).map(candidateItem)) }
        )

        let album = Data("/music/Album".utf8)
        slot.changeSelection(.toggle(add: [album], remove: []))
        var entered = toggleEntered.stream.makeAsyncIterator()
        await entered.next()
        slot.selectAllShown()
        slot.setTab(.done)
        releaseToggle.continuation.yield(())
        try await Wait.until { writes.all.count == 2 }

        #expect(writes.all == ["toggle", "select all"])
    }

    /// Folding is core's list's: the slot tells it which group to fold or
    /// open, and folds every group in one call.
    @Test("folding a group asks core's list to fold it")
    func foldingAsksCoresList() async throws {
        let folds = CallLog<String>()
        let group = BridgeFolderReleaseDecisionKey(
            watchedFolderPath: Data("/library".utf8),
            relativeFolder: Data("Box".utf8)
        )
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: ImportSelection(),
            makeHandle: { _ in
                .stub(
                    setGroupFolded: { group, folded in
                        let folder =
                            String(bytes: group.relativeFolder, encoding: .utf8)
                            ?? "not UTF-8"
                        folds.record("\(folder) \(folded)")
                    },
                    setAllGroupsFolded: { folded in
                        folds.record("all \(folded)")
                    }
                )
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list != nil }

        slot.setGroupExpanded(group, false)
        try await Wait.until { folds.all.count == 1 }
        slot.setGroupExpanded(group, true)
        try await Wait.until { folds.all.count == 2 }
        slot.setAllGroupsExpanded(false)
        try await Wait.until { folds.all.count == 3 }

        #expect(folds.all == ["Box true", "Box false", "all true"])
    }
}

/// A view change: the write that sets it, the windows read after it and the
/// row the table shows.
@MainActor
@Suite("Import list slot view changes")
struct ImportListSlotViewChangeTests {
    /// Another tab is another view: until the write that sets it answers and
    /// the windows read after it land, the rows the list holds are the tab's
    /// before it, which the slot does not take as this one's.
    @Test("a new tab's rows are the list's only once its windows land")
    func aNewTabIsAnsweredByItsOwnRead() async throws {
        let delivery = AppliedViewResolver()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: ImportSelection(),
            makeHandle: { _ in
                .stub(
                    items: (0..<3).map(candidateItem),
                    setView: { view in
                        await delivery.wait(for: view)
                        return nil
                    }
                )
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list != nil }
        #expect(slot.answersView)

        slot.setTab(.done)
        #expect(!slot.answersView)
        try await Wait.until { !delivery.requests.isEmpty }
        #expect(delivery.requests.last?.tab == .done)

        delivery.resolve()
        try await Wait.until { slot.answersView }
    }

    /// The write that sets a view answers where it opens; the windows are
    /// read again from there, and the table is told which row to show.
    @Test("a view change opens its windows at the row the write answers")
    func aViewChangeOpensAtItsOpening() async throws {
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: ImportSelection(),
            makeHandle: { _ in
                .stub(
                    items: (0..<80).map(candidateItem),
                    setView: { _ in
                        .at(
                            key: BridgeListQueries<BridgeImportListItem>
                                .fixedKey(60),
                            position: 60
                        )
                    }
                )
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(0) != nil }
        #expect(slot.opened.position == 0)

        slot.setTab(.done)
        try await Wait.until { slot.answersView && slot.opened.position == 60 }

        #expect(slot.opened.narrowing.tab == .done)
        #expect(slot.list?.row(at: 60) != nil)
        #expect(slot.list?.row(at: 0) == nil)
    }

    /// The slot asks core for the list under the entry the person chose, and
    /// keeps it as the sidebar's own.
    @Test("a chosen entry narrows the view")
    func aChosenEntryNarrowsTheView() async throws {
        let requests = RecordedImportView()
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: { view in
                requests.set(view)
                return .stub(setView: {
                    requests.set($0)
                    return nil
                })
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list != nil }
        #expect(requests.last?.pendingFilter == BridgePendingFilter.all)

        slot.setPendingFilter(.needsYou)
        try await Wait.until {
            requests.last?.pendingFilter == BridgePendingFilter.needsYou
        }
        #expect(uiStore.importCandidatePendingFilter == .needsYou)

        slot.setPendingFilter(.all)
        try await Wait.until {
            requests.last?.pendingFilter == BridgePendingFilter.all
        }
        #expect(uiStore.importCandidatePendingFilter == .all)
    }
}

/// Where core says the candidate `key` sits in the fixed list.
private func located(
    _ key: Data,
    at position: Int,
    tab: BridgeTriageTab
) -> BridgeImportCandidateListLocation {
    BridgeImportCandidateListLocation(
        stableKey: candidateStableKey(key),
        tab: tab,
        groupKey: nil,
        visiblePosition: UInt64(position),
        key: BridgeListQueries<BridgeImportListItem>.fixedKey(position)
    )
}

final class CandidatePlacementNavigationTests: XCTestCase {
    @MainActor
    func testRevealFollowsCurrentPlacementBeforeLoading() async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        uiStore.setImportCandidateFilterText("hidden")
        let targetKey = candidateKey(61)
        let delivery = AppliedViewResolver()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: { _ in
                .stub(
                    items: (0..<80).map(candidateItem),
                    setView: { view in
                        await delivery.wait(for: view)
                        return nil
                    },
                    locateCandidate: { key in
                        located(key, at: 61, tab: .done)
                    }
                )
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(0) != nil }
        let outcome = CandidateRevealOutcome()
        Task {
            outcome.position = try? await slot.revealCandidate(targetKey)
        }
        try await Wait.until { !delivery.requests.isEmpty }

        XCTAssertEqual(uiStore.importCandidateTab, .done)
        XCTAssertTrue(uiStore.importCandidateFilterText.isEmpty)
        XCTAssertNil(outcome.position)
        XCTAssertNil(slot.list?.idAt(61))
        XCTAssertEqual(delivery.requests.first?.tab, .done)
        XCTAssertEqual(delivery.requests.first?.filterText.isEmpty, true)

        delivery.resolve()
        try await Wait.until { outcome.position != nil }

        XCTAssertEqual(outcome.position, 61)
        XCTAssertEqual(
            slot.list?.idAt(61),
            candidateStableKey(targetKey)
        )
    }
}

@MainActor
@Observable
private final class CandidateRevealOutcome {
    var position: Int?
}

/// The order the import list was last sorted by, as the slot reads it back.
@MainActor
@Suite("Import list slot saved order")
struct ImportListSlotSavedOrderTests {
    @Test("a saved order that doesn't decode sorts newest first")
    func unreadableSortPreference() throws {
        let suite = "ImportListSlotTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        defaults.set("oldestFirst", forKey: "importCandidateSortOrder")

        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: ImportSelection(),
            defaults: defaults,
            makeHandle: { _ in .failing() }
        )

        #expect(slot.sortOrder == .newestFirst)
    }
}
