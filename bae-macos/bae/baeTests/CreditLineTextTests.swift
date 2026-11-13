import BaeKit
import Testing

struct CreditLineTextTests {
    private func line(_ names: [(String, BridgeCreditJoin?)])
        -> BridgeCreditLine
    {
        BridgeCreditLine(
            names: names.map { BridgeCreditedName(name: $0.0, join: $0.1) }
        )
    }

    @Test(
        "a recognized join is worded from the catalog and a plain run is a list"
    )
    func recognizedJoinThenPlainRun() {
        let credits = line([
            ("Artist A", .featuring),
            ("Artist B", nil),
            ("Artist C", nil),
            ("Artist D", nil),
        ])
        #expect(
            credits.text == "Artist A feat. Artist B, Artist C, and Artist D"
        )
    }

    @Test("names with no join are joined by the list formatter")
    func plainNames() {
        let credits = line([("Artist A", nil), ("Artist B", nil)])
        #expect(credits.text == "Artist A and Artist B")
    }

    @Test("a join bae does not recognize is shown as written")
    func writtenJoin() {
        let credits = line([
            ("Artist A", .unrecognized(text: " / ")),
            ("Artist B", nil),
        ])
        #expect(credits.text == "Artist A / Artist B")
    }

    @Test("each recognized join has its own wording")
    func everyRecognizedJoin() {
        let joins: [(BridgeCreditJoin, String)] = [
            (.featuring, "Artist A feat. Artist B"),
            (.ampersand, "Artist A & Artist B"),
            (.and, "Artist A and Artist B"),
            (.with, "Artist A with Artist B"),
            (.versus, "Artist A vs. Artist B"),
        ]
        for (join, expected) in joins {
            let credits = line([("Artist A", join), ("Artist B", nil)])
            #expect(credits.text == expected)
        }
    }

    @Test("one name is itself and no names are empty")
    func edges() {
        #expect(BridgeCreditLine.named("Artist A").text == "Artist A")
        #expect(BridgeCreditLine(names: []).text == "")
    }
}
