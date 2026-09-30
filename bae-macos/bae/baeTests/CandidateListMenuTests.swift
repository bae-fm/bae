import BaeKit
import XCTest

@testable import bae

final class CandidateListMenuTests: XCTestCase {
    @MainActor
    func testScanProgressDoesNotReplaceTheOpenMenu() {
        XCTAssertEqual(
            menu(status: .scanning(foundCount: 1)),
            menu(status: .scanning(foundCount: 2))
        )
    }

    @MainActor
    func testChangedScanFailureReplacesTheMenuContent() {
        XCTAssertNotEqual(
            menu(status: .failed(error: "First failure")),
            menu(status: .failed(error: "Second failure"))
        )
    }

    @MainActor
    private func menu(
        status: BridgeFolderScanStatus,
        sortOrder: BridgeImportListOrder = .newestFirst,
        pendingFilter: BridgePendingFilter = .all,
        pendingFilterEntries: [BridgePendingFilterEntry] =
            PreviewData.pendingFilterEntries()
    ) -> CandidateListMenu {
        CandidateListMenu(
            watchedFolders: [
                BridgeWatchedFolder(path: "/Imports", name: "Imports")
            ],
            refreshingFolders: [],
            scanStatuses: ["/Imports": status],
            networkFolders: [],
            hasGroups: false,
            sortOrder: sortOrder,
            onSetSortOrder: { _ in },
            pendingFilter: pendingFilter,
            pendingFilterEntries: { pendingFilterEntries },
            pendingFilterApplies: true,
            onSetPendingFilter: { _ in },
            onAddFolder: {},
            onSetAllGroupsExpanded: { _ in },
            onRefreshFolder: { _ in },
            onRemoveFolder: { _ in }
        )
    }

    @MainActor
    func testChangedSortReplacesTheMenuCheckmark() {
        XCTAssertNotEqual(
            menu(status: .complete),
            menu(status: .complete, sortOrder: .oldestFirst)
        )
    }

    @MainActor
    func testChangedPendingFilterReplacesTheMenuCheckmark() {
        XCTAssertNotEqual(
            menu(status: .complete),
            menu(status: .complete, pendingFilter: .identified)
        )
    }

    /// The entries are counted when the menu opens, so a count moving while
    /// it is open — a run starting or ending — leaves it standing.
    @MainActor
    func testChangedEntryCountDoesNotReplaceTheOpenMenu() {
        var counted = PreviewData.pendingFilterEntries()
        counted[1] = BridgePendingFilterEntry(
            filter: counted[1].filter,
            count: counted[1].count + 1,
            selectable: true
        )
        XCTAssertEqual(
            menu(status: .complete),
            menu(status: .complete, pendingFilterEntries: counted)
        )
    }
}
