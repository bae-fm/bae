import BaeKit
import Testing

struct AlbumRatingTextTests {
    @Test(
        "the line names this person first, then each other rater by name and stars"
    )
    func ratersLine() {
        let raters: [BridgeRater] = [
            .you, .person(name: "Ana", stars: .five),
        ]
        #expect(raters.text == "You · Ana rated it 5")
    }

    @Test("an album nobody rated has no line")
    func noRaters() {
        let raters: [BridgeRater] = []
        #expect(raters.text.isEmpty)
    }

    @Test("the heart line names this person first, then each other by name")
    func heartersLine() {
        let hearters: [BridgeHearter] = [.you, .person(name: "Ana")]
        #expect(hearters.text == "You · Ana hearted it")
    }

    @Test("an album nobody hearted has no heart line")
    func noHearters() {
        let hearters: [BridgeHearter] = []
        #expect(hearters.text.isEmpty)
    }
}
