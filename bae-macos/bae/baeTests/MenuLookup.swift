import AppKit
import Testing

@testable import bae

/// Items of a menu found by their actions, its submenus' included.
@MainActor
enum MenuLookup {
    static func submenu(_ menu: NSMenu, at index: Int) throws -> NSMenu {
        try #require(menu.items[index].submenu)
    }

    /// Every item under `menu`, its submenus' included.
    static func allItems(_ menu: NSMenu) -> [NSMenuItem] {
        menu.items.flatMap { item in
            [item] + (item.submenu.map(allItems) ?? [])
        }
    }

    static func items(_ menu: NSMenu, _ action: String) -> [NSMenuItem] {
        allItems(menu).filter { $0.action == Selector(action) }
    }

    static func item(_ menu: NSMenu, _ action: String) throws
        -> NSMenuItem
    {
        try #require(items(menu, action).first, "\(action)")
    }

    static func expect(
        _ menu: NSMenu,
        _ actions: [String],
        sourceLocation: SourceLocation = #_sourceLocation
    ) {
        let present = Set(allItems(menu).compactMap(\.action))
        for action in actions {
            #expect(
                present.contains(Selector(action)),
                "\(menu.title) has \(action)",
                sourceLocation: sourceLocation
            )
        }
    }
}
