import BaeKit
import Foundation
import Testing

@testable import bae

/// The generic `[Criterion].toJSON()/fromJSON()` round trip backs
/// UserDefaults persistence for all three library sort modes. One suite
/// exercises the shared implementation against each criterion type rather
/// than duplicating the same assertions per mode.
struct SortCriteriaCodableTests {
    @Test("album criteria round-trip through JSON")
    func albumRoundTrip() throws {
        let criteria = [
            BridgeSortCriterion(field: .artist, direction: .descending),
            BridgeSortCriterion(field: .year, direction: .ascending),
        ]

        let data = try #require(criteria.toJSON())
        let decoded = try #require([BridgeSortCriterion].fromJSON(data))

        #expect(decoded == criteria)
    }

    @Test("composer criteria round-trip through JSON")
    func composerRoundTrip() throws {
        let criteria = [
            BridgeComposerSortCriterion(
                field: .workCount,
                direction: .descending
            )
        ]

        let data = try #require(criteria.toJSON())
        let decoded = try #require(
            [BridgeComposerSortCriterion].fromJSON(data)
        )

        #expect(decoded == criteria)
    }

    @Test("artist criteria round-trip through JSON")
    func artistRoundTrip() throws {
        let criteria = [
            BridgeArtistSortCriterion(
                field: .albumCount,
                direction: .ascending
            )
        ]

        let data = try #require(criteria.toJSON())
        let decoded = try #require([BridgeArtistSortCriterion].fromJSON(data))

        #expect(decoded == criteria)
    }

    @Test("an unknown field key is skipped, not crashed on")
    func unknownFieldSkipped() throws {
        let data = try #require(
            """
            [{"field": "notARealField", "direction": "ascending"}]
            """.data(using: .utf8)
        )

        let decoded = try #require([BridgeSortCriterion].fromJSON(data))

        #expect(decoded.isEmpty)
    }

    @Test("an unknown direction key is skipped, not crashed on")
    func unknownDirectionSkipped() throws {
        let data = try #require(
            """
            [{"field": "name", "direction": "sideways"}]
            """.data(using: .utf8)
        )

        let decoded = try #require(
            [BridgeComposerSortCriterion].fromJSON(data)
        )

        #expect(decoded.isEmpty)
    }

    @Test("malformed JSON fails closed to nil, not a crash")
    func malformedJSONFailsClosed() throws {
        let data = try #require("not json".data(using: .utf8))

        #expect([BridgeSortCriterion].fromJSON(data) == nil)
    }
}
