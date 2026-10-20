import AppKit
import BaeKit
import Testing

@testable import bae

/// Found's filter in the candidate list's menu, as its controller builds it
/// on opening: core's entries in core's order, each with its count, the
/// chosen one checked.
@MainActor
@Suite("Found's filter menu")
struct PendingFilterMenuTests {
    final class Recorder {
        var chosen: [BridgePendingFilter] = []
    }

    private func entry(
        _ filter: BridgePendingFilter,
        _ count: UInt32
    ) -> BridgePendingFilterEntry {
        BridgePendingFilterEntry(
            filter: filter,
            count: count,
            selectable: count > 0
        )
    }

    /// Every entry, In Progress holding nothing.
    private var entries: [BridgePendingFilterEntry] {
        [
            entry(.all, 30),
            entry(.needsYou, 9),
            entry(.inProgress, 0),
            entry(.identified, 14),
            entry(.unmatched, 3),
            entry(.notLookedUp, 4),
        ]
    }

    /// The menu's controller, its menu as it opens with `selected` chosen.
    private func opened(
        _ selected: BridgePendingFilter,
        recorder: Recorder = Recorder()
    ) -> CandidateListMenuController {
        let controller = CandidateListMenuController(
            state: CandidateListMenuState(
                watchedFolders: [],
                refreshingFolders: [],
                scanStatuses: [:],
                networkFolders: [],
                hasGroups: false,
                sortOrder: .newestFirst,
                pendingFilter: selected,
                pendingFilterEntries: entries,
                pendingFilterApplies: true
            ),
            actions: CandidateListMenuActions(
                onSetSortOrder: { _ in },
                onSetPendingFilter: { recorder.chosen.append($0) },
                onAddFolder: {},
                onSetAllGroupsExpanded: { _ in },
                onRefreshFolder: { _ in },
                onRemoveFolder: { _ in }
            )
        )
        controller.menuNeedsUpdate(controller.menu)
        return controller
    }

    private func title(_ entry: BridgePendingFilterEntry) -> String {
        entry.filter.label(count: entry.count)
    }

    private func position(
        of entry: BridgePendingFilterEntry,
        in menu: NSMenu
    ) -> Int? {
        menu.items.firstIndex { $0.title == title(entry) }
    }

    /// Where `filter`'s entry sits in the menu.
    private func position(
        of filter: BridgePendingFilter,
        in menu: NSMenu
    ) -> Int? {
        entries.first { $0.filter == filter }
            .flatMap { position(of: $0, in: menu) }
    }

    @Test("core's entries list in core's order, each with its count")
    func entriesListInCoreOrderWithCounts() {
        let expected = entries.map(title)
        let controller = opened(.all)
        #expect(
            controller.menu.items.map(\.title).filter(expected.contains)
                == expected
        )
    }

    @Test("the chosen entry is checked, and no other")
    func theChosenEntryIsChecked() throws {
        let controller = opened(.identified)
        let menu = controller.menu
        for entry in entries {
            let index = try #require(position(of: entry, in: menu))
            #expect(
                menu.items[index].state
                    == (entry.filter == .identified ? .on : .off)
            )
        }
    }

    @Test("an entry holding no row cannot be chosen")
    func anEmptyEntryIsDisabled() throws {
        let controller = opened(.all)
        let menu = controller.menu
        for entry in entries {
            let index = try #require(position(of: entry, in: menu))
            #expect(menu.items[index].isEnabled == entry.selectable)
        }
    }

    @Test("a chosen entry holding no row stays chosen")
    func aChosenEmptyEntryStaysChecked() throws {
        let controller = opened(.inProgress)
        let menu = controller.menu
        let index = try #require(
            position(of: .inProgress, in: menu)
        )
        #expect(menu.items[index].state == .on)
    }

    @Test("choosing an entry says which it is")
    func choosingAnEntrySendsIt() throws {
        let recorder = Recorder()
        let controller = opened(.all, recorder: recorder)
        let menu = controller.menu
        let index = try #require(position(of: .needsYou, in: menu))
        withExtendedLifetime(controller) {
            menu.performActionForItem(at: index)
        }
        #expect(recorder.chosen == [.needsYou])
    }

    @Test("choosing the chosen entry again changes nothing")
    func choosingTheChosenEntryAgainSendsNothing() throws {
        let recorder = Recorder()
        let controller = opened(.identified, recorder: recorder)
        let menu = controller.menu
        let index = try #require(
            position(of: .identified, in: menu)
        )
        withExtendedLifetime(controller) {
            menu.performActionForItem(at: index)
        }
        #expect(recorder.chosen.isEmpty)
    }
}
