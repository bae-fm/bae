import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A pill's field menu re-points its criterion in place. The list is where
/// that write lands, so the rule lives on the list.
@Suite("Re-pointing a sort pill")
struct SortCriteriaRowTests {
    @Test("a re-pointed criterion keeps its place and direction")
    func rePointingKeepsPlaceAndDirection() {
        var criteria = [
            BridgeSortCriterion(field: .dateAdded, direction: .descending),
            BridgeSortCriterion(field: .title, direction: .descending),
        ]

        criteria.replaceField(.title, with: .artist)

        #expect(
            criteria == [
                BridgeSortCriterion(field: .dateAdded, direction: .descending),
                BridgeSortCriterion(field: .artist, direction: .descending),
            ]
        )
    }

    @Test("a field another pill sorts by is refused")
    func aTakenFieldIsRefused() {
        let before = [
            BridgeSortCriterion(field: .dateAdded, direction: .descending),
            BridgeSortCriterion(field: .title, direction: .ascending),
        ]
        var criteria = before

        criteria.replaceField(.title, with: .dateAdded)
        criteria.replaceField(.title, with: .title)
        criteria.replaceField(.year, with: .artist)

        #expect(criteria == before)
    }

    /// Every field's name resolves through the string catalog: read in
    /// Japanese, where every one of these words differs, none stays English.
    @Test("every sort field's name is translated")
    func everyFieldNameIsTranslated() {
        func read(_ name: LocalizedStringResource, in language: String)
            -> String
        {
            var name = name
            name.locale = Locale(identifier: language)
            return String(localized: name)
        }
        let names =
            BridgeSortField.allCases.map(\.displayName)
            + BridgeComposerSortField.allCases.map(\.displayName)
            + BridgeArtistSortField.allCases.map(\.displayName)
        for name in names {
            let english = read(name, in: "en")
            #expect(
                read(name, in: "ja") != english,
                "\(english) reads the same in Japanese"
            )
        }
    }

    /// The pill's field is drawn as its menu button's label and listed in the
    /// menu it opens, so assert that menu still lists its field.
    @MainActor
    @Test("the pill draws its field")
    func thePillDrawsItsField() async throws {
        let size = NSSize(width: 240, height: 44)
        try await SnapshotTestSupport.withHostedWindow(
            SortCriterionPill(
                criterion: .constant(
                    BridgeSortCriterion(
                        field: .dateAdded,
                        direction: .descending
                    )
                ),
                takenFields: [],
                canRemove: false,
                onSetField: { _ in },
                onRemove: {}
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let menu = try #require(HostedMenus.controls(in: host).first)
            let field = String(localized: BridgeSortField.dateAdded.displayName)
            let titles = try #require(try HostedMenus.titles(of: menu))
            #expect(titles.contains(field), "the menu lists \(titles)")
        }
    }
}
