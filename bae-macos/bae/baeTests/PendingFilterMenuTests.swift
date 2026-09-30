import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Found's filter in the candidate menu, as AppKit builds it from the SwiftUI
/// menu content: core's entries in core's order, each with its count, the
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

    private func menu(
        _ selected: BridgePendingFilter,
        recorder: Recorder = Recorder()
    ) -> NSMenu {
        let menu = NSHostingMenu(
            rootView: PendingFilterSection(
                entries: { entries },
                selected: selected,
                onSelect: { recorder.chosen.append($0) }
            )
        )
        menu.update()
        return menu
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
        #expect(
            menu(.all).items.map(\.title).filter(expected.contains) == expected
        )
    }

    @Test("the chosen entry is checked, and no other")
    func theChosenEntryIsChecked() throws {
        let menu = menu(.identified)
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
        let menu = menu(.all)
        for entry in entries {
            let index = try #require(position(of: entry, in: menu))
            #expect(menu.items[index].isEnabled == entry.selectable)
        }
    }

    @Test("a chosen entry holding no row stays chosen")
    func aChosenEmptyEntryStaysChecked() throws {
        let menu = menu(.inProgress)
        let index = try #require(
            position(of: .inProgress, in: menu)
        )
        #expect(menu.items[index].state == .on)
    }

    @Test("choosing an entry says which it is")
    func choosingAnEntrySendsIt() throws {
        let recorder = Recorder()
        let menu = menu(.all, recorder: recorder)
        let index = try #require(position(of: .needsYou, in: menu))
        menu.performActionForItem(at: index)
        #expect(recorder.chosen == [.needsYou])
    }

    @Test("choosing the chosen entry again changes nothing")
    func choosingTheChosenEntryAgainSendsNothing() throws {
        let recorder = Recorder()
        let menu = menu(.identified, recorder: recorder)
        let index = try #require(
            position(of: .identified, in: menu)
        )
        menu.performActionForItem(at: index)
        #expect(recorder.chosen.isEmpty)
    }
}
