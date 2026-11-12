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
        /// The menu's controller, which its items' actions point back to.
        var controller: CandidateListMenuController?
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

    /// The menu's items as it opens with `selected` chosen.
    private func opened(
        _ selected: BridgePendingFilter,
        recorder: Recorder = Recorder()
    ) -> [MacMenuItem] {
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
        recorder.controller = controller
        return controller.entries.compactMap { entry in
            if case .item(let item) = entry { return item }
            return nil
        }
    }

    private func title(_ entry: BridgePendingFilterEntry) -> String {
        entry.filter.label(count: entry.count)
    }

    /// The item of `entry`.
    private func item(
        of entry: BridgePendingFilterEntry,
        in items: [MacMenuItem]
    ) -> MacMenuItem? {
        items.first { $0.title == title(entry) }
    }

    /// The item of `filter`'s entry.
    private func item(
        of filter: BridgePendingFilter,
        in items: [MacMenuItem]
    ) -> MacMenuItem? {
        entries.first { $0.filter == filter }
            .flatMap { item(of: $0, in: items) }
    }

    @Test("core's entries list in core's order, each with its count")
    func entriesListInCoreOrderWithCounts() {
        let expected = entries.map(title)
        let items = opened(.all)
        #expect(items.map(\.title).filter(expected.contains) == expected)
    }

    @Test("the chosen entry is checked, and no other")
    func theChosenEntryIsChecked() throws {
        let items = opened(.identified)
        for entry in entries {
            let found = try #require(item(of: entry, in: items))
            #expect(found.isChecked == (entry.filter == .identified))
        }
    }

    @Test("an entry holding no row cannot be chosen")
    func anEmptyEntryIsDisabled() throws {
        let items = opened(.all)
        for entry in entries {
            let found = try #require(item(of: entry, in: items))
            #expect(found.isDisabled == !entry.selectable)
        }
    }

    @Test("a chosen entry holding no row stays chosen")
    func aChosenEmptyEntryStaysChecked() throws {
        let items = opened(.inProgress)
        let found = try #require(
            item(of: BridgePendingFilter.inProgress, in: items)
        )
        #expect(found.isChecked)
    }

    @Test("choosing an entry says which it is")
    func choosingAnEntrySendsIt() throws {
        let recorder = Recorder()
        let items = opened(.all, recorder: recorder)
        let needsYou = try #require(
            item(of: BridgePendingFilter.needsYou, in: items)
        )
        needsYou.action()
        #expect(recorder.chosen == [.needsYou])
    }

    @Test("choosing the chosen entry again changes nothing")
    func choosingTheChosenEntryAgainSendsNothing() throws {
        let recorder = Recorder()
        let items = opened(.identified, recorder: recorder)
        let identified = try #require(
            item(of: BridgePendingFilter.identified, in: items)
        )
        identified.action()
        #expect(recorder.chosen.isEmpty)
    }
}
