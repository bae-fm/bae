import BaeKit
import Foundation
import Testing
import XCTest

@testable import bae

private struct ReadFailed: Error {}

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
            firstUnidentifiedPosition: { _, _ in nil },
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

private final class CandidatePositionResolver: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Int?, Never>?
    private var requested: [(BridgeImportListView, String)] = []

    var requests: [(BridgeImportListView, String)] {
        lock.withLock { requested }
    }

    func position(
        view: BridgeImportListView,
        candidateKey: String
    ) async -> Int? {
        await withCheckedContinuation { continuation in
            lock.withLock {
                requested.append((view, candidateKey))
                self.continuation = continuation
            }
        }
    }

    func resolve(_ position: Int?) {
        let continuation = lock.withLock {
            let continuation = self.continuation
            self.continuation = nil
            return continuation
        }
        continuation?.resume(returning: position)
    }
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
            skipAction: .unskip,
            matched: nil,
            metadataSummary: nil,
            coverThumbnail: nil,
            selectable: false,
            importStatus: nil,
            metadataProvenance: nil
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
    @Test("a failed first page becomes the slot's failure and an alert")
    func aFailedFirstPageIsSurfaced() async {
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            makeSource: { _ in FailingPageSource().pages },
            locateCandidate: { _, _ in nil }
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

    @Test("explicit reveal waits for its view before loading the target page")
    func explicitRevealWaitsForViewDelivery() async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.done)
        uiStore.setImportCandidateFilterText("hidden")
        let resolver = CandidatePositionResolver()
        let items = (0..<80).map(candidateItem)
        let pageSource = ImportListPreviewPageSource(items: items)
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            makeSource: { _ in
                ImportListPages(
                    source: pageSource,
                    setView: { _ in },
                    firstUnidentifiedPosition: { view, target in
                        await resolver.position(
                            view: view,
                            candidateKey: target.candidateKey
                        )
                    },
                    waitForView: { _ in }
                )
            },
            locateCandidate: { _, _ in nil }
        )
        slot.startLoad()
        await waitUntil { slot.list?.idAt(0) != nil }
        let list = try #require(slot.list)
        let target = BridgeFirstUnidentifiedRowRef(
            candidateKey: candidateKey(61),
            stableKey: "candidate:\(candidateKey(61))",
            groupKey: nil,
            visiblePosition: nil
        )
        let outcome = CandidateRevealOutcome()
        Task {
            outcome.position = try? await slot.reveal(target)
        }
        await waitUntil { !resolver.requests.isEmpty }

        #expect(outcome.position == nil)
        #expect(list.idAt(61) == nil)
        let requested = try #require(resolver.requests.first)
        #expect(requested.0.tab == .pending)
        #expect(requested.0.filterText.isEmpty)
        #expect(requested.1 == target.candidateKey)

        resolver.resolve(61)
        await waitUntil { outcome.position != nil }

        #expect(outcome.position == 61)
        #expect(list.idAt(61) == target.stableKey)
    }

    @Test("explicit reveal does not navigate to a mismatched delivered row")
    func explicitRevealRejectsMismatchedDelivery() async throws {
        let uiStore = UiStore()
        let resolver = CandidatePositionResolver()
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
                    firstUnidentifiedPosition: { view, target in
                        await resolver.position(
                            view: view,
                            candidateKey: target.candidateKey
                        )
                    },
                    waitForView: { _ in }
                )
            },
            locateCandidate: { _, _ in nil }
        )
        slot.startLoad()
        await waitUntil { slot.list?.idAt(0) != nil }
        let target = BridgeFirstUnidentifiedRowRef(
            candidateKey: "/library/missing",
            stableKey: "candidate:/library/missing",
            groupKey: nil,
            visiblePosition: nil
        )
        let task = Task { try await slot.reveal(target) }
        await waitUntil { !resolver.requests.isEmpty }

        resolver.resolve(61)

        #expect(try await task.value == nil)
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
                    firstUnidentifiedPosition: { _, _ in nil },
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
            }
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
