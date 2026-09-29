import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The filter list in the candidate menu, as AppKit builds it from the
/// SwiftUI menu content: All, then core's groups, each set apart by a
/// separator, every entry checked or not.
@MainActor
@Suite("The Pending filter menu")
struct PendingFilterMenuTests {
    /// What one entry of the menu was told to do.
    enum Sent: Equatable {
        case set(BridgePendingState, Bool)
        case showAll
    }

    final class Recorder {
        var sent: [Sent] = []
    }

    private func menu(
        _ filters: [BridgePendingState],
        recorder: Recorder = Recorder()
    ) -> NSMenu {
        let menu = NSHostingMenu(
            rootView: PendingFilterSection(
                filters: filters,
                onSetFilter: { recorder.sent.append(.set($0, $1)) },
                onShowAll: { recorder.sent.append(.showAll) }
            )
        )
        menu.update()
        return menu
    }

    private func index(of title: String, in menu: NSMenu) -> Int {
        menu.items.firstIndex { $0.title == title } ?? -1
    }

    @Test("core's groups are set apart by separators, in core's order")
    func groupsAreSeparated() {
        let items = menu([]).items
            .map { item in
                item.isSeparatorItem ? "—" : item.title
            }
        let expected =
            [String(localized: "All")]
            + PendingFilterSection.groups.flatMap { group in
                ["—"] + group.map(\.label)
            }
        // The section sits under its title.
        let section = Array(
            items.drop { $0 != String(localized: "All") }.prefix(expected.count)
        )
        #expect(section == expected)
    }

    @Test("All is checked while no state narrows the rows")
    func allIsCheckedWhenNothingNarrows() {
        let menu = menu([])
        #expect(
            menu.items[index(of: String(localized: "All"), in: menu)].state
                == .on
        )
        for filter in PendingFilterSection.groups.joined() {
            #expect(menu.items[index(of: filter.label, in: menu)].state == .off)
        }
    }

    @Test("every checked state is checked, and All is not")
    func checkedStatesAreChecked() {
        let checked: [BridgePendingState] = [.needsYou, .importError]
        let menu = menu(checked)
        #expect(
            menu.items[index(of: String(localized: "All"), in: menu)].state
                == .off
        )
        for filter in PendingFilterSection.groups.joined() {
            #expect(
                menu.items[index(of: filter.label, in: menu)].state
                    == (checked.contains(filter) ? .on : .off)
            )
        }
    }

    @Test("choosing a state says which it is and whether it is now checked")
    func choosingAStateSendsWhatThePersonDid() {
        let recorder = Recorder()
        let menu = menu([.needsYou], recorder: recorder)
        menu.performActionForItem(
            at: index(of: BridgePendingState.identified.label, in: menu)
        )
        menu.performActionForItem(
            at: index(of: BridgePendingState.needsYou.label, in: menu)
        )
        #expect(
            recorder.sent == [.set(.identified, true), .set(.needsYou, false)]
        )
    }

    @Test("choosing All while narrowed shows every row")
    func choosingAllShowsEveryRow() {
        let recorder = Recorder()
        let menu = menu([.lookupError], recorder: recorder)
        menu.performActionForItem(
            at: index(of: String(localized: "All"), in: menu)
        )
        #expect(recorder.sent == [.showAll])
    }
}
