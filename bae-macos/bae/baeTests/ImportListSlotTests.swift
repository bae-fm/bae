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

@MainActor
private func waitUntil(_ predicate: @MainActor () -> Bool) async {
    for _ in 0..<500 {
        if predicate() { return }
        await Task.yield()
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
            resolvedBoundaries: [],
            combineAncestorKey: nil,
            actionable: true,
            placement: .skipped,
            readyCheck: nil,
            actionBasis: BridgeCandidateActionBasis(
                actionable: true,
                placement: .skipped,
                lookupFailed: false
            ),
            matched: nil,
            metadataSummary: nil,
            coverThumbnail: nil,
            selectable: false,
            importStatus: nil,
            metadataProvenance: nil,
            reading: .unidentified,
        ),
        isGroupMember: false
    )
}

private func candidateKey(_ index: Int) -> String {
    "/library/release-\(index)"
}

/// The import tab decides between three panes — the list, the "add a folder"
/// prompt, and the read failure — before one is drawn, so it has to know that
/// its first page read failed.
///
/// `PaginatedList` does not hand a first-page failure to `onError`: it keeps it
/// as `initialLoadError` for a list view to render inline, which is where every
/// other list surface reads it. The import tab read only `onError`, so the one
/// failure that matters at launch went nowhere and a library nobody could look
/// at rendered as a library with no folders.
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
        await waitUntil { slot.list != nil }
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
            await waitUntil { reopened.list != nil }
            #expect(requests.last?.order == order)
        }
    }

    @Test("a failed first page becomes the slot's failure and an alert")
    func aFailedFirstPageIsSurfaced() async {
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            makeSource: { _ in FailingPageSource().pages },
            locateCandidate: { _, _ in nil },
            firstIdentifyingCandidate: { _ in nil }
        )

        #expect(slot.loadFailure == nil)
        #expect(uiStore.lastError == nil)

        slot.startLoad()
        await waitUntil { slot.loadFailure != nil }

        #expect(slot.loadFailure != nil)
        // The same failure is raised as the global alert, the way every other
        // background failure reaches the person.
        #expect(uiStore.lastError != nil)
    }

    /// Which candidate the count is still waiting on is core's answer, asked
    /// when the person goes to it; the slot then follows that candidate's
    /// placement like any other reveal.
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
        await waitUntil { slot.list?.idAt(0) != nil }

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
        await waitUntil { slot.list?.idAt(0) != nil }

        #expect(try await slot.revealFirstIdentifying() == nil)
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
        await waitUntil { slot.list?.idAt(0) != nil }
        let outcome = CandidateRevealOutcome()
        Task {
            outcome.position = try? await slot.revealCandidate(targetKey)
        }
        await waitUntil { !delivery.requests.isEmpty }

        XCTAssertEqual(uiStore.importCandidateTab, .done)
        XCTAssertTrue(uiStore.importCandidateFilterText.isEmpty)
        XCTAssertNil(outcome.position)
        XCTAssertNil(slot.list?.idAt(61))
        XCTAssertEqual(delivery.requests.first?.tab, .done)
        XCTAssertEqual(delivery.requests.first?.filterText.isEmpty, true)

        delivery.resolve()
        await waitUntil { outcome.position != nil }

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
