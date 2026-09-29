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
        pendingFilters: [BridgePendingState] = []
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
            pendingFilters: pendingFilters,
            pendingFilterApplies: true,
            onSetPendingFilter: { _, _ in },
            onShowAllPending: {},
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
            menu(status: .complete, pendingFilters: [.identified])
        )
        XCTAssertNotEqual(
            menu(status: .complete, pendingFilters: [.needsYou]),
            menu(status: .complete, pendingFilters: [.needsYou, .importError])
        )
    }
}
