import BaeKit
import Testing

@testable import bae

struct TrackRateMenuTests {
    private func submenu(
        _ stars: BridgeStars?,
        onRate: @escaping (BridgeStars?) -> Void = { _ in }
    ) throws -> [MacMenuEntry] {
        let entries = TrackRateMenu(stars: stars, onRate: onRate).entries
        guard
            case .submenu(let title, let items, _) = try #require(
                entries.first
            )
        else {
            Issue.record("Rate is not a submenu")
            return []
        }
        #expect(title == "Rate")
        return items
    }

    @Test("it lists five stars down to one, the person's own checked")
    func listsStars() throws {
        let items = try submenu(.four)
        let rows: [(String, Bool)] = items.compactMap {
            guard case .item(let item) = $0 else { return nil }
            return (item.title, item.isChecked)
        }
        #expect(
            rows.map(\.0) == [
                "★★★★★", "★★★★", "★★★", "★★", "★", "Clear rating",
            ]
        )
        #expect(rows.map(\.1) == [false, true, false, false, false, false])
    }

    @Test("Clear rating is off for an unrated track and clears a rated one")
    func clearRating() throws {
        var rated: [BridgeStars?] = []
        let unrated = try submenu(nil)
        guard case .item(let off) = try #require(unrated.last) else { return }
        #expect(off.isDisabled)
        let items = try submenu(.two) { rated.append($0) }
        guard case .item(let clear) = try #require(items.last) else { return }
        #expect(!clear.isDisabled)
        clear.action()
        guard case .item(let five) = try #require(items.first) else { return }
        five.action()
        #expect(rated == [nil, .five])
    }
}
