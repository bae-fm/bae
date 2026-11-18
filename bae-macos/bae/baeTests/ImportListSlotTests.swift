import BaeKit
import Foundation
import Observation
import Testing
import XCTest

@testable import bae

private struct ReadFailed: Error {}

extension ImportListHandle {
    /// A handle over `items` whose reads are the given stand-ins, and change
    /// nothing otherwise: a view opens at the top, a fold hides nothing, and
    /// no candidate is found.
    static func stub(
        items: [BridgeImportListItem] = [],
        queries:
            (
                @Sendable (BridgeImportListRequest) -> BridgeListQueries<
                    BridgeImportListItem
                >
            )? = nil,
        viewChange:
            @escaping @Sendable (BridgeImportListRequest, [Data]) async throws
            -> BridgeImportListViewChange = { _, _ in
                BridgeImportListViewChange(opening: .top, left: [])
            },
        hiddenByFold:
            @escaping @Sendable (
                BridgeImportListRequest, BridgeImportListRequest, [Data]
            ) async throws -> [Data] = { _, _, _ in [] },
        groupKeys:
            @escaping @Sendable () async throws
            -> [BridgeFolderReleaseDecisionKey] = { [] },
        locateCandidate:
            @escaping @Sendable (Data) async throws
            -> BridgeImportCandidateListLocation? = { _ in nil },
        candidatePosition:
            @escaping @Sendable (BridgeImportListRequest, Data) async throws
            -> UInt64? = { _, _ in nil },
        firstIdentifyingCandidate:
            @escaping @Sendable (BridgeImportListOrder) async throws -> Data? =
            {
                _ in nil
            }
    ) -> ImportListHandle {
        let fixed = ImportListHandle.fixed(items: items, tab: .pending)
        return ImportListHandle(
            queries: queries ?? fixed.queries,
            summary: fixed.summary,
            filterEntries: fixed.filterEntries,
            selection: fixed.selection,
            viewChange: viewChange,
            hiddenByFold: hiddenByFold,
            groupKeys: groupKeys,
            locateCandidate: locateCandidate,
            candidatePosition: candidatePosition,
            firstIdentifyingCandidate: firstIdentifyingCandidate
        )
    }

    /// A handle whose every window read fails.
    static func failing() -> ImportListHandle {
        stub(queries: { _ in
            .failing(ReadFailed(), answers: UnansweredCount())
        })
    }
}

private final class RecordedImportRequest: @unchecked Sendable {
    private let lock = NSLock()
    private var request: BridgeImportListRequest?

    var last: BridgeImportListRequest? {
        lock.withLock { request }
    }

    func set(_ request: BridgeImportListRequest) {
        lock.recording { self.request = request }
    }
}

/// A read that waits until the test lets it answer.
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
            actionable: true,
            placement: .skipped,
            live: BridgeCandidateLiveState(
                identifying: nil,
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
            reading: .unidentified
        ),
        group: nil
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
        let requests = RecordedImportRequest()
        func makeSlot() -> ImportListSlot {
            ImportListSlot(
                importStore: ImportStore(),
                uiStore: UiStore(),
                selection: ImportSelection(),
                defaults: defaults,
                makeHandle: {
                    .stub(queries: { request in
                        requests.set(request)
                        return ImportListHandle.fixed(items: [], tab: .pending)
                            .queries(request)
                    })
                }
            )
        }
        let slot = makeSlot()
        #expect(slot.sortOrder == .newestFirst)
        slot.startLoad()
        try await Wait.until { slot.list != nil }
        #expect(requests.last?.view.order == .newestFirst)

        for order: BridgeImportListOrder in [
            .oldestFirst, .pathDescending, .pathAscending, .newestFirst,
        ] {
            slot.setSortOrder(order)
            #expect(slot.sortOrder == order)
            try await Wait.until { requests.last?.view.order == order }
            let reopened = makeSlot()
            #expect(reopened.sortOrder == order)
            reopened.startLoad()
            try await Wait.until { reopened.list != nil }
            #expect(requests.last?.view.order == order)
        }
    }

    @Test("a failed first window becomes the slot's failure and an alert")
    func aFailedFirstWindowIsSurfaced() async throws {
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: { .failing() }
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
            makeHandle: {
                .stub(
                    items: (0..<80).map(candidateItem),
                    locateCandidate: { _ in located(on: .pending) },
                    candidatePosition: { _, _ in 61 },
                    firstIdentifyingCandidate: { _ in target }
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
            makeHandle: {
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

    /// A view change sends core the selected keys and removes from the
    /// selection the keys it returns; a fold does the same with the keys of
    /// the rows it hid.
    @Test("a view change and a fold take out the selected rows they hide")
    func viewChangesAndFoldsTakeOutWhatTheyHide() async throws {
        let first = candidateKey(1)
        let second = candidateKey(2)
        let third = candidateKey(3)
        let sent = CallLog<Set<Data>>()
        let selection = ImportSelection()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: selection,
            makeHandle: {
                .stub(
                    items: (0..<5).map(candidateItem),
                    viewChange: { _, selected in
                        sent.record(Set(selected))
                        return BridgeImportListViewChange(
                            opening: .top,
                            left: [first]
                        )
                    },
                    hiddenByFold: { _, _, selected in
                        sent.record(Set(selected))
                        return [second]
                    },
                    groupKeys: { [boxGroup] }
                )
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list != nil }
        selection.select([first, second, third])

        slot.setTab(.done)
        try await Wait.until { selection.keys == [second, third] }
        slot.setAllGroupsExpanded(false)
        try await Wait.until { selection.keys == [third] }
        #expect(sent.all == [[first, second, third], [second, third]])
    }

    /// Folds are the slot's: each fold opens the list under a request that
    /// holds the folded groups, and folding every group folds the groups core
    /// names.
    @Test("folding a group reads the list under the groups it folds")
    func foldingReadsTheListUnderItsFolds() async throws {
        let folds = CallLog<String>()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: ImportSelection(),
            makeHandle: {
                .stub(
                    hiddenByFold: { _, after, _ in
                        folds.record(
                            after.folded
                                .map {
                                    String(
                                        bytes: $0.relativeFolder,
                                        encoding: .utf8
                                    )
                                        ?? "not UTF-8"
                                }
                                .joined(separator: ",")
                        )
                        return []
                    },
                    groupKeys: { [boxGroup] }
                )
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list != nil }

        slot.setGroupExpanded(boxGroup, false)
        try await Wait.until { folds.all.count == 1 }
        slot.setGroupExpanded(boxGroup, true)
        try await Wait.until { folds.all.count == 2 }
        slot.setAllGroupsExpanded(false)
        try await Wait.until { folds.all.count == 3 }

        #expect(folds.all == ["Box", "", "Box"])
    }
}

/// The group a test folds.
private let boxGroup = BridgeFolderReleaseDecisionKey(
    watchedFolderPath: Data("/library".utf8),
    relativeFolder: Data("Box".utf8)
)

/// A view change: the read of where it opens, the list read after it and the
/// rows the table shows.
@MainActor
@Suite("Import list slot view changes")
struct ImportListSlotViewChangeTests {
    /// Another tab is another request: until core answers where it opens and
    /// the list read under it has its rows and count, the slot keeps the
    /// previous tab's list on screen.
    @Test("the sidebar keeps the previous list until the new tab's list reads")
    func aNewTabKeepsThePreviousListUntilItsOwnReads() async throws {
        let delivery = AppliedViewResolver()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: ImportSelection(),
            makeHandle: {
                .stub(
                    items: (0..<3).map(candidateItem),
                    viewChange: { request, _ in
                        await delivery.wait(for: request.view)
                        return BridgeImportListViewChange(
                            opening: .top,
                            left: []
                        )
                    }
                )
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list != nil }
        let previous = slot.list

        slot.setTab(.done)
        try await Wait.until { !delivery.requests.isEmpty }
        #expect(delivery.requests.last?.tab == .done)
        #expect(slot.list === previous)
        #expect(slot.shown?.view.tab == .pending)

        delivery.resolve()
        try await Wait.until { slot.shown?.view.tab == .done }
        #expect(slot.list !== previous)
    }

    /// Core returns the row where a view opens; the slot reads the new list
    /// from there, sends the row as an opening and takes it as the person's
    /// place.
    @Test("a view change opens its list at the row core returns")
    func aViewChangeOpensAtItsOpening() async throws {
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: ImportSelection(),
            makeHandle: {
                .stub(
                    items: (0..<80).map(candidateItem),
                    viewChange: { _, _ in
                        BridgeImportListViewChange(
                            opening: .at(position: 60),
                            left: []
                        )
                    }
                )
            }
        )
        var opened: [Int] = []
        let listening = slot.openings.sink { opened.append($0) }
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(0) != nil }
        #expect(opened == [0])

        slot.setTab(.done)
        try await Wait.until {
            slot.shown?.view.tab == .done && opened == [0, 60]
        }

        #expect(slot.place?.position == 60)
        #expect(slot.list?.row(at: 60) != nil)
        #expect(slot.list?.row(at: 0) == nil)
        listening.cancel()
    }

    /// For another order the slot asks core nothing and sends no opening: the
    /// person's place stays the same while the table draws the rows in the new
    /// order.
    @Test("another order opens the list without an opening")
    func anotherOrderOpensNothing() async throws {
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: ImportSelection(),
            makeHandle: {
                .stub(
                    items: (0..<80).map(candidateItem),
                    viewChange: { _, _ in
                        Issue.record("another order asks for no opening")
                        return BridgeImportListViewChange(
                            opening: .top,
                            left: []
                        )
                    }
                )
            }
        )
        var opened: [Int] = []
        let listening = slot.openings.sink { opened.append($0) }
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(0) != nil }
        let place = ImportListPlace(row: .atPosition, position: 30, offset: 4)
        slot.place = place

        slot.setSortOrder(.oldestFirst)
        try await Wait.until { slot.shown?.view.order == .oldestFirst }

        #expect(opened == [0])
        #expect(slot.place == place)
        listening.cancel()
    }

    /// The slot reads the list under the entry the person chose, and keeps it
    /// as the sidebar's own.
    @Test("a chosen entry narrows the view")
    func aChosenEntryNarrowsTheView() async throws {
        let requests = RecordedImportRequest()
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: {
                .stub(queries: { request in
                    requests.set(request)
                    return ImportListHandle.fixed(items: [], tab: .pending)
                        .queries(request)
                })
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list != nil }
        #expect(requests.last?.view.pendingFilter == BridgePendingFilter.all)

        slot.setPendingFilter(.needsYou)
        try await Wait.until {
            requests.last?.view.pendingFilter == BridgePendingFilter.needsYou
        }
        #expect(uiStore.importCandidatePendingFilter == .needsYou)

        slot.setPendingFilter(.all)
        try await Wait.until {
            requests.last?.view.pendingFilter == BridgePendingFilter.all
        }
        #expect(uiStore.importCandidatePendingFilter == .all)
    }
}

/// Where core says a candidate in no group is revealed: on `tab`.
private func located(
    on tab: BridgeTriageTab
) -> BridgeImportCandidateListLocation {
    BridgeImportCandidateListLocation(tab: tab, groupKey: nil)
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
            makeHandle: {
                .stub(
                    items: (0..<80).map(candidateItem),
                    locateCandidate: { _ in located(on: .done) },
                    candidatePosition: { request, _ in
                        await delivery.wait(for: request.view)
                        return 61
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

extension CandidatePlacementNavigationTests {
    @MainActor
    func testRevealOfACandidateTheViewNoLongerShowsGoesNowhere() async throws {
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: {
                .stub(
                    items: (0..<80).map(candidateItem),
                    locateCandidate: { _ in located(on: .pending) },
                    candidatePosition: { _, _ in nil }
                )
            }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(0) != nil }

        let position = try await slot.revealCandidate(candidateKey(61))

        XCTAssertNil(position)
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
            makeHandle: { .failing() }
        )

        #expect(slot.sortOrder == .newestFirst)
    }
}
