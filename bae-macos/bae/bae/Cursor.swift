import Foundation

/// A validated cursor over a non-empty list of identifiable items. The cursor
/// always points to a real item; mutations preserve that invariant. Views read
/// `current`, `items`, `canCycle` and a lightbox's `place`, and call
/// `select(id:)`.
struct Cursor<Item: Identifiable & Equatable>: Equatable {
    let items: [Item]
    private(set) var index: Int

    /// Fails when `items` is empty. When `preferring` is supplied and matches
    /// an item id, that item becomes the focus; otherwise the first item is.
    /// A non-matching `preferring` is not an error — the preferred item may
    /// have legitimately disappeared.
    init?(items: [Item], preferring id: Item.ID? = nil) {
        guard !items.isEmpty else {
            return nil
        }
        self.items = items
        if let id, let i = items.firstIndex(where: { $0.id == id }) {
            index = i
        }
        else {
            index = 0
        }
    }

    var current: Item {
        items[index]
    }

    var canCycle: Bool {
        items.count > 1
    }

    mutating func select(id: Item.ID) {
        if let i = items.firstIndex(where: { $0.id == id }) {
            index = i
        }
    }

}

extension Cursor where Item: LightboxImage {
    /// The lightbox's place at the cursor: the items before and after the
    /// current one, wrapping around the ends, and none for a lone item.
    var place: LightboxPlace<Item> {
        guard canCycle else {
            return LightboxPlace(
                items: items,
                current: current,
                previous: nil,
                next: nil
            )
        }
        return LightboxPlace(
            items: items,
            current: current,
            previous: items[(index - 1 + items.count) % items.count].id,
            next: items[(index + 1) % items.count].id
        )
    }
}
