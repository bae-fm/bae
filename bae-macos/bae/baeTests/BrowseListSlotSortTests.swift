import BaeKit
import Foundation
import Testing

@testable import bae

/// A browse slot saves its sort criteria in UserDefaults as the JSON of the
/// bridge's own sort types, and reads them back when the slot is made again.
/// A saved value it can't read whole is replaced by the default sort, never
/// by the part of it that decodes.
@MainActor
@Suite("Browse list slot sort persistence")
struct BrowseListSlotSortTests {
    private static let key = "sortCriteria"
    private static let defaultCriteria = [
        BridgeSortCriterion(field: .dateAdded, direction: .descending)
    ]

    private func makeSlot(
        _ defaults: UserDefaults
    ) -> BrowseListSlot<BridgeAlbum, BridgeSortCriterion> {
        BrowseListSlot(
            defaults: defaults,
            defaultsKey: Self.key,
            defaultCriteria: Self.defaultCriteria,
            makePageSource: { _ in AlbumPreviewPageSource(albums: []) },
            onError: { _ in }
        )
    }

    private func withDefaults(
        _ body: (UserDefaults) throws -> Void
    ) throws {
        let suite = "BrowseListSlotSortTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        try body(defaults)
    }

    @Test("nothing saved sorts by the default")
    func nothingSaved() throws {
        try withDefaults { defaults in
            #expect(makeSlot(defaults).sortCriteria == Self.defaultCriteria)
        }
    }

    @Test("chosen criteria survive making the slot again")
    func criteriaPersist() throws {
        try withDefaults { defaults in
            let criteria = [
                BridgeSortCriterion(field: .artist, direction: .descending),
                BridgeSortCriterion(field: .year, direction: .ascending),
            ]
            makeSlot(defaults).setSortCriteria(criteria)

            #expect(makeSlot(defaults).sortCriteria == criteria)
        }
    }

    @Test("an unknown field fails the whole saved list, not just its entry")
    func unknownFieldFailsWhole() throws {
        try withDefaults { defaults in
            let saved = """
                [
                  {"field": {"title": {}}, "direction": {"ascending": {}}},
                  {"field": {"notARealField": {}}, "direction": {"ascending": {}}}
                ]
                """
            defaults.set(Data(saved.utf8), forKey: Self.key)

            #expect(makeSlot(defaults).sortCriteria == Self.defaultCriteria)
        }
    }

    @Test("criteria saved as plain field and direction names sort by default")
    func plainNamesFail() throws {
        try withDefaults { defaults in
            let saved = """
                [{"field": "title", "direction": "ascending"}]
                """
            defaults.set(Data(saved.utf8), forKey: Self.key)

            #expect(makeSlot(defaults).sortCriteria == Self.defaultCriteria)
        }
    }

    @Test("a saved value that isn't data sorts by the default")
    func notData() throws {
        try withDefaults { defaults in
            defaults.set("title", forKey: Self.key)

            #expect(makeSlot(defaults).sortCriteria == Self.defaultCriteria)
        }
    }

    @Test("an empty saved list sorts by the default")
    func emptyList() throws {
        try withDefaults { defaults in
            defaults.set(Data("[]".utf8), forKey: Self.key)

            #expect(makeSlot(defaults).sortCriteria == Self.defaultCriteria)
        }
    }
}
