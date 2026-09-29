import BaeKit
import Foundation
import Testing
import XCTest

@testable import bae

private struct ReadFailed: Error {}

private final class RecordedImportView: @unchecked Sendable {
    private let lock = NSLock()
    private var view: BridgeImportListView?

    var last: BridgeImportListView? {
        lock.withLock { view }
    }

    func set(_ view: BridgeImportListView) {
        lock.withLock { self.view = view }
    }
}

/// A source whose every page fails immediately.
private struct FailingPageSource: PageSource {
    typealias Row = BridgeImportListItem

    func subscribe(
        offset _: Int,
        limit _: Int,
        onValue _: @escaping @MainActor @Sendable ([Row], Int) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        let task = Task { @MainActor in onError(ReadFailed()) }
        return TaskBackedSubscription(task: task)
    }

    var pages: ImportListPages {
        ImportListPages(
            source: self,
            setView: { _ in },
            waitForView: { _ in }
        )
    }
}

private final class TaskBackedSubscription: PageSubscription,
    @unchecked Sendable
{
    private let task: Task<Void, Never>

    init(task: Task<Void, Never>) { self.task = task }

    func cancel() { task.cancel() }
}

private final class AppliedViewResolver: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, Never>?
    private var requested: [BridgeImportListView] = []

    var requests: [BridgeImportListView] {
        lock.withLock { requested }
    }

    func wait(for view: BridgeImportListView) async {
        await withCheckedContinuation { continuation in
            lock.withLock {
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
        stableKey: "candidate:\(key)",
        row: BridgeTriageRow(
            candidateKey: key,
            folderName: "Release \(index)",
            watchedFolderPath: "/library",
            displayPath: "Release \(index)",
            actionable: true,
            placement: .skipped,
            actionBasis: BridgeCandidateActionBasis(
                actionable: true,
                placement: .skipped,
                draftValid: false,
                lookup: nil,
                separable: false,
                standing: .notLookedUp
            ),
            matched: nil,
            metadataSummary: nil,
            cover: nil,
            importStatus: nil,
            metadataProvenance: nil,
            reading: .unidentified,
            selected: false
        ),
        isGroupMember: false
    )
}

private func candidateKey(_ index: Int) -> String {
    "/library/release-\(index)"
}

/// The import tab picks its pane before drawing one, so a failed first page
/// read has to reach it as `initialLoadError`.
@MainActor
@Suite("Import list slot read failures")
struct ImportListSlotTests {
    @Test("sort choices update the source and survive recreating the slot")
    func sortPreferenceReconfiguresAndPersists() async throws {
        let suite = "ImportListSlotTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let requests = RecordedImportView()
        let source = ImportListPreviewPageSource(items: [])
        func makeSlot() -> ImportListSlot {
            ImportListSlot(
                importStore: ImportStore(),
                uiStore: UiStore(),
                selection: ImportSelection(),
                defaults: defaults,
                makeSource: { view in
                    requests.set(view)
                    return ImportListPages(
                        source: source,
                        setView: { requests.set($0) },
                        waitForView: { _ in }
                    )
                },
                locateCandidate: { _, _ in nil },
                firstIdentifyingCandidate: { _ in nil }
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
            #expect(requests.last?.order == order)
            let reopened = makeSlot()
            #expect(reopened.sortOrder == order)
            reopened.startLoad()
            try await Wait.until { reopened.list != nil }
            #expect(requests.last?.order == order)
        }
    }

    @Test("a failed first page becomes the slot's failure and an alert")
    func aFailedFirstPageIsSurfaced() async throws {
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeSource: { _ in FailingPageSource().pages },
            locateCandidate: { _, _ in nil },
            firstIdentifyingCandidate: { _ in nil }
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
        let pageSource = ImportListPreviewPageSource(
            items: (0..<80).map(candidateItem)
        )
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeSource: { _ in
                ImportListPages(
                    source: pageSource,
                    setView: { _ in },
                    waitForView: { _ in }
                )
            },
            locateCandidate: { _, key in
                BridgeImportCandidateListLocation(
                    stableKey: "candidate:\(key)",
                    tab: .pending,
                    groupKey: nil,
                    visiblePosition: 61
                )
            },
            firstIdentifyingCandidate: { _ in target }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(0) != nil }

        let revealed = try await slot.revealFirstIdentifying()

        #expect(revealed?.candidateKey == target)
        #expect(revealed?.position == 61)
        #expect(uiStore.importCandidateTab == .pending)
        #expect(uiStore.importCandidateFilterText.isEmpty)
        #expect(slot.list?.idAt(61) == "candidate:\(target)")
    }

    @Test("nothing identifying reveals nothing")
    func revealFirstIdentifyingWithNothingIdentifying() async throws {
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeSource: { _ in
                ImportListPreviewPageSource(
                    items: (0..<80).map(candidateItem)
                )
                .pages
            },
            locateCandidate: { _, _ in
                Issue.record("nothing to locate")
                return nil
            },
            firstIdentifyingCandidate: { _ in nil }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(0) != nil }

        #expect(try await slot.revealFirstIdentifying() == nil)
    }

    /// Each selection write waits for the ones before it, so core applies
    /// them in the order the person made them; a view change asks core to
    /// keep only the rows the new view shows.
    @Test("selection writes land in order, and a view change keeps shown rows")
    func selectionWritesLandInOrder() async throws {
        let writes = CallLog<String>()
        let toggleEntered = AsyncStream<Void>.makeStream()
        let releaseToggle = AsyncStream<Void>.makeStream()
        let selection = ImportSelection(
            operations: .stub(
                change: { _, _ in
                    toggleEntered.continuation.yield(())
                    for await _ in releaseToggle.stream { break }
                    writes.record("toggle")
                    return 1
                },
                selectAll: { _ in writes.record("select all") },
                keepShown: { view in
                    writes.record("keep shown in \(view.tab)")
                }
            )
        )
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: UiStore(),
            selection: selection,
            makeSource: { _ in
                ImportListPreviewPageSource(
                    items: (0..<80).map(candidateItem)
                )
                .pages
            },
            locateCandidate: { _, _ in nil },
            firstIdentifyingCandidate: { _ in nil }
        )

        slot.changeSelection(to: ["/music/Album"], from: [], by: .toggle)
        var entered = toggleEntered.stream.makeAsyncIterator()
        await entered.next()
        slot.selectAllShown()
        slot.setTab(.done)
        releaseToggle.continuation.yield(())
        try await Wait.until { writes.all.count == 3 }

        #expect(writes.all == ["toggle", "select all", "keep shown in done"])
    }

    /// The slot sends which state the person checked or cleared, and the
    /// view it asks for is what core makes of that: states in the menu's
    /// order, and every state checked the same as none.
    @Test("checked states narrow the view as core decides")
    func checkedStatesNarrowTheViewAsCoreDecides() async throws {
        let requests = RecordedImportView()
        let uiStore = UiStore()
        let source = ImportListPreviewPageSource(items: [])
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeSource: { view in
                requests.set(view)
                return ImportListPages(
                    source: source,
                    setView: { requests.set($0) },
                    waitForView: { _ in }
                )
            },
            locateCandidate: { _, _ in nil },
            firstIdentifyingCandidate: { _ in nil }
        )
        slot.startLoad()
        try await Wait.until { slot.list != nil }

        slot.setPendingFilter(.importError, checked: true)
        slot.setPendingFilter(.needsYou, checked: true)
        #expect(requests.last?.pendingFilters == [.needsYou, .importError])
        #expect(
            uiStore.importCandidatePendingFilters == [.needsYou, .importError]
        )

        slot.setPendingFilter(.importError, checked: false)
        #expect(requests.last?.pendingFilters == [.needsYou])

        for filter in PendingFilterSection.groups.joined() {
            slot.setPendingFilter(filter, checked: true)
        }
        #expect(requests.last?.pendingFilters == [])
        #expect(uiStore.importCandidatePendingFilters.isEmpty)

        slot.setPendingFilter(.identified, checked: true)
        slot.showAllPending()
        #expect(requests.last?.pendingFilters == [])
    }

}

final class CandidatePlacementNavigationTests: XCTestCase {
    @MainActor
    func testRevealFollowsCurrentPlacementBeforeLoading() async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        uiStore.setImportCandidateFilterText("hidden")
        let targetKey = candidateKey(61)
        let items = (0..<80).map(candidateItem)
        let pageSource = ImportListPreviewPageSource(items: items)
        let delivery = AppliedViewResolver()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            selection: ImportSelection(),
            makeSource: { _ in
                ImportListPages(
                    source: pageSource,
                    setView: { _ in },
                    waitForView: { view in
                        await delivery.wait(for: view)
                    }
                )
            },
            locateCandidate: { _, key in
                BridgeImportCandidateListLocation(
                    stableKey: "candidate:\(key)",
                    tab: .done,
                    groupKey: nil,
                    visiblePosition: 61
                )
            },
            firstIdentifyingCandidate: { _ in nil }
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
            "candidate:\(targetKey)"
        )
    }
}

@MainActor
private final class CandidateRevealOutcome {
    var position: Int?
}
