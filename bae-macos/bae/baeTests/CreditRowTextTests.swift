import BaeKit
import Testing

struct CreditRowTextTests {
    @Test("a row covering every track names only its artists")
    func everyTrack() {
        let row = BridgeCreditRow(
            role: .word("Producer"),
            artists: .named("Artist Name"),
            tracks: []
        )
        #expect(row.text == "Artist Name")
    }

    @Test("a row covering some tracks names them after its artists")
    func someTracks() {
        let row = BridgeCreditRow(
            role: .word("Arranger"),
            artists: .named("Artist Name"),
            tracks: ["2", "5"]
        )
        #expect(row.text == "Artist Name (tracks 2 and 5)")
    }
}
