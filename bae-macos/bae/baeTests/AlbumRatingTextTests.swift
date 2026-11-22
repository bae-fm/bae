import BaeKit
import Testing

struct AlbumRatingTextTests {
    @Test(
        "the line names this person first, then each other rater by name and stars"
    )
    func ratersLine() {
        let rating = BridgeAlbumRating(
            mine: .four,
            raters: [.you, .person(name: "Ana", stars: .five)]
        )
        #expect(rating.ratersText == "You · Ana rated it 5")
    }

    @Test("an album nobody rated has no line")
    func noRaters() {
        let rating = BridgeAlbumRating(
            mine: nil,
            raters: []
        )
        #expect(rating.ratersText.isEmpty)
    }

    @Test("tapping a star the person already gave clears the rating")
    func tappingTheGivenStarClears() {
        let rating = BridgeAlbumRating(
            mine: .four,
            raters: []
        )
        #expect(rating.stars(afterTapping: .two) == .two)
        #expect(rating.stars(afterTapping: .four) == nil)
    }
}

struct HeartTests {
    @Test("three stars or more is a heart")
    func threeStarsAreAHeart() {
        #expect(!BridgeStars.two.isHeart)
        #expect(BridgeStars.three.isHeart)
        #expect(BridgeStars.five.isHeart)
    }

    @Test("Heart stores five stars, and Remove heart clears the rating")
    func heartTogglesFiveStarsAndNone() {
        #expect(BridgeStars.afterHeartToggle(from: nil) == .five)
        #expect(BridgeStars.afterHeartToggle(from: .one) == .five)
        #expect(BridgeStars.afterHeartToggle(from: .three) == nil)
        let hearted = BridgeAlbumRating(mine: .four, raters: [.you])
        #expect(hearted.isHearted)
        #expect(hearted.starsAfterHeartToggle == nil)
        let unhearted = BridgeAlbumRating(mine: .two, raters: [.you])
        #expect(!unhearted.isHearted)
        #expect(unhearted.starsAfterHeartToggle == .five)
    }

    @Test("the line names who hearted the album, this person first")
    func heartersLine() {
        let rating = BridgeAlbumRating(
            mine: .four,
            raters: [
                .you, .person(name: "Ana", stars: .five),
                .person(name: "Ben", stars: .two),
            ]
        )
        #expect(rating.heartersText == "You · Ana hearted it")
    }

    @Test("a rating under three stars names nobody")
    func lowRatingsNameNobody() {
        let rating = BridgeAlbumRating(
            mine: .one,
            raters: [.you, .person(name: "Ben", stars: .two)]
        )
        #expect(rating.heartersText.isEmpty)
    }
}
