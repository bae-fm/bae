import BaeKit
import Foundation
import Testing

@testable import bae

/// The candidate list's menu: the filter's counts move in the lines of the
/// menu on screen, and what else changes waits for the next opening.
@MainActor
@Suite("Candidate list menu")
struct CandidateListMenuTests {
    private func state(
        entries: [BridgePendingFilterEntry],
        scanStatus: BridgeFolderScanStatus = .complete
    ) -> CandidateListMenuState {
        CandidateListMenuState(
            watchedFolders: [
                BridgeWatchedFolder(
                    path: Data("/Imports".utf8),
                    pathText: "/Imports",
                    name: "Imports"
                )
            ],
            refreshingFolders: [],
            scanStatuses: [Data("/Imports".utf8): scanStatus],
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
        in lines: [MacMenuEntry]
    ) -> MacMenuItem? {
        for case .item(let item) in lines
        where item.title == entry.filter.label(count: entry.count) {
            return item
        }
        return nil
    }

    /// A count moving retitles and re-enables the filter's lines.
    @Test("the filter's counts move in the menu's lines")
    func theCountsMoveInTheMenu() throws {
        let idle = entries(inProgress: 0)
        let controller = CandidateListMenuController(
            state: state(entries: idle),
            actions: actions
        )
        let inProgress = try #require(item(idle[1], in: controller.entries))
        #expect(inProgress.isDisabled)

        let running = entries(inProgress: 1)
        controller.show(state(entries: running))

        let moved = try #require(item(running[1], in: controller.entries))
        #expect(!moved.isDisabled)
        #expect(item(running[2], in: controller.entries) != nil)
    }

    /// The first delivery's entries fill the Filter section, after its
    /// header.
    @Test("entries arriving fill the Filter section")
    func entriesArrivingFillTheFilterSection() throws {
        let controller = CandidateListMenuController(
            state: state(entries: []),
            actions: actions
        )

        let delivered = entries(inProgress: 0)
        controller.show(state(entries: delivered))

        let titles = controller.entries.compactMap { entry -> String? in
            if case .header(let title) = entry { return title }
            return entry.title
        }
        let header = try #require(titles.firstIndex(of: "Filter"))
        #expect(
            Array(titles[(header + 1)...(header + delivered.count)])
                == delivered.map { $0.filter.label(count: $0.count) }
        )
    }

    /// A folder's scan state is drawn as the menu opens.
    @Test("a scan's state reaches the folder's line")
    func aScanStateReachesTheFoldersLine() throws {
        let controller = CandidateListMenuController(
            state: state(entries: entries(inProgress: 0)),
            actions: actions
        )
        func accessory() -> MacMenuAccessory? {
            for case .submenu("Imports", _, let accessory) in controller.entries
            {
                return accessory
            }
            return nil
        }
        #expect(accessory() == nil)

        controller.show(
            state(
                entries: entries(inProgress: 0),
                scanStatus: .failed(error: "The volume could not be reached.")
            )
        )

        #expect(accessory()?.help == "The volume could not be reached.")
    }
}
