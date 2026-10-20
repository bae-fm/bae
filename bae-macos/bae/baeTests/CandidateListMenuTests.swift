import AppKit
import BaeKit
import Testing

@testable import bae

/// The candidate list's menu while it is open: the filter's counts move in
/// the items on screen, and what else changes waits for the next opening.
@MainActor
@Suite("Candidate list menu")
struct CandidateListMenuTests {
    private func state(
        entries: [BridgePendingFilterEntry],
        scanStatus: BridgeFolderScanStatus = .complete
    ) -> CandidateListMenuState {
        CandidateListMenuState(
            watchedFolders: [
                BridgeWatchedFolder(path: "/Imports", name: "Imports")
            ],
            refreshingFolders: [],
            scanStatuses: ["/Imports": scanStatus],
            networkFolders: [],
            hasGroups: false,
            sortOrder: .newestFirst,
            pendingFilter: .all,
            pendingFilterEntries: entries,
            pendingFilterApplies: true
        )
    }

    private var actions: CandidateListMenuActions {
        CandidateListMenuActions(
            onSetSortOrder: { _ in },
            onSetPendingFilter: { _ in },
            onAddFolder: {},
            onSetAllGroupsExpanded: { _ in },
            onRefreshFolder: { _ in },
            onRemoveFolder: { _ in }
        )
    }

    private func entries(inProgress: UInt32) -> [BridgePendingFilterEntry] {
        [
            BridgePendingFilterEntry(filter: .all, count: 3, selectable: true),
            BridgePendingFilterEntry(
                filter: .inProgress,
                count: inProgress,
                selectable: inProgress > 0
            ),
            BridgePendingFilterEntry(
                filter: .notLookedUp,
                count: 3 - inProgress,
                selectable: inProgress < 3
            ),
        ]
    }

    /// The item titled with `entry`'s label and count.
    private func item(
        _ entry: BridgePendingFilterEntry,
        in menu: NSMenu
    ) -> NSMenuItem? {
        menu.items.first {
            $0.title == entry.filter.label(count: entry.count)
        }
    }

    /// A count moving while the menu is open retitles and re-enables the
    /// filter items on screen, the same items in the same menu: the menu is
    /// not built again, so it stays open under the pointer.
    @Test("the filter's counts move in the open menu's items")
    func theCountsMoveInTheOpenMenu() throws {
        let idle = entries(inProgress: 0)
        let controller = CandidateListMenuController(
            state: state(entries: idle),
            actions: actions
        )
        controller.menuNeedsUpdate(controller.menu)
        let opened = controller.menu.items
        let inProgress = try #require(item(idle[1], in: controller.menu))
        #expect(!inProgress.isEnabled)

        let running = entries(inProgress: 1)
        controller.show(state(entries: running))

        #expect(controller.menu.items == opened, "the menu was built again")
        #expect(
            inProgress.title == BridgePendingFilter.inProgress.label(count: 1)
        )
        #expect(inProgress.isEnabled)
        #expect(item(running[2], in: controller.menu) != nil)
    }

    /// The first delivery's entries reach a menu opened before it, after the
    /// Filter header.
    @Test("entries arriving while the menu is open fill its Filter section")
    func entriesArrivingWhileOpenFillTheFilterSection() throws {
        let controller = CandidateListMenuController(
            state: state(entries: []),
            actions: actions
        )
        controller.menuNeedsUpdate(controller.menu)

        let delivered = entries(inProgress: 0)
        controller.show(state(entries: delivered))

        let titles = controller.menu.items.map(\.title)
        let header = try #require(titles.firstIndex(of: "Filter"))
        #expect(
            Array(titles[(header + 1)...(header + delivered.count)])
                == delivered.map { $0.filter.label(count: $0.count) }
        )
    }

    /// A folder's scan state is drawn as the menu opens: a change while it is
    /// open is in the next opening.
    @Test("a scan's state reaches the folder's entry at the next opening")
    func aScanStateReachesTheNextOpening() throws {
        let controller = CandidateListMenuController(
            state: state(entries: entries(inProgress: 0)),
            actions: actions
        )
        controller.menuNeedsUpdate(controller.menu)
        let folder = try #require(
            controller.menu.items.first { $0.title == "Imports" }
        )
        #expect(folder.toolTip == nil)

        controller.show(
            state(
                entries: entries(inProgress: 0),
                scanStatus: .failed(error: "The volume could not be reached.")
            )
        )
        controller.menuNeedsUpdate(controller.menu)

        let reopened = try #require(
            controller.menu.items.first { $0.title == "Imports" }
        )
        #expect(reopened.toolTip == "The volume could not be reached.")
    }
}
