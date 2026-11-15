import BaeKit
import Testing

struct AlbumRatingTextTests {
    @Test("the line names this person first, then each other by name and stars")
    func ratersLine() {
        let rating = BridgeAlbumRating(
            canRate: true,
            shown: true,
            mine: .four,
            raters: [.you, .person(name: "Ana", stars: .five)]
        )
        #expect(rating.ratersText == "You · Ana rated it 5")
    }

    @Test("an album nobody rated has no line")
    func noRaters() {
        let rating = BridgeAlbumRating(
            canRate: true,
            shown: true,
            mine: nil,
            raters: []
        )
        #expect(rating.ratersText.isEmpty)
    }

    @Test("tapping a star the person already gave clears the rating")
    func tappingTheGivenStarClears() {
        let rating = BridgeAlbumRating(
            canRate: true,
            shown: true,
            mine: .four,
            raters: []
        )
        #expect(rating.stars(afterTapping: 2) == .two)
        #expect(rating.stars(afterTapping: 4) == nil)
    }
}
