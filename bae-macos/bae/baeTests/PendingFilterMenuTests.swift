import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The filter list in the candidate menu, as AppKit builds it from the
/// SwiftUI menu content: All, then core's groups, each set apart by a
/// separator.
@MainActor
@Suite("The Pending filter menu")
struct PendingFilterMenuTests {
    @Test("core's groups are set apart by separators, in core's order")
    func groupsAreSeparated() {
        let menu = NSHostingMenu(
            rootView: PendingFilterPicker(selection: nil, onSelect: { _ in })
        )
        menu.update()
        let items = menu.items.map { item in
            item.isSeparatorItem ? "—" : item.title
        }
        let expected =
            [String(localized: "All")]
            + PendingFilterPicker.groups.flatMap { group in
                ["—"] + group.map(\.label)
            }
        // The inline picker is its own section of the menu, under its title.
        let section = Array(
            items.drop { $0 != String(localized: "All") }.prefix(expected.count)
        )
        #expect(section == expected)
    }
}
