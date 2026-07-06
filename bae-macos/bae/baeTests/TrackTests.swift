import Testing

@testable import bae

struct TrackTests {
    @Test("track position text handles missing track numbers")
    func positionTextHandlesMissingTrackNumbers() {
        #expect(
            BridgeTrackPosition.sided(sideLetter: "A", number: nil).positionText
                == "A"
        )
        #expect(
            BridgeTrackPosition.disc(disc: 1, number: nil).positionText == "1-"
        )
        #expect(BridgeTrackPosition.flat(number: nil).positionText == "")
    }

    @Test("track position text preserves numbered positions")
    func positionTextPreservesNumberedPositions() {
        #expect(
            BridgeTrackPosition.sided(sideLetter: "B", number: 3).positionText
                == "B3"
        )
        #expect(
            BridgeTrackPosition.disc(disc: 2, number: 4).positionText == "2-4"
        )
        #expect(BridgeTrackPosition.flat(number: 5).positionText == "5")
    }
}
