import BaeKit
import Testing

@testable import bae

struct TrackTests {
    @Test("track stores core-rendered position text")
    func storesCoreRenderedPositionText() {
        let track = Track(
            from: BridgeTrack(
                id: "track-1",
                title: "Track Title",
                durationMs: 180_000,
                durationClock: bridgeLengthClock(ms: 180_000),
                artistNames: "Artist Name",
                displayArtist: nil,
                positionText: "A"
            )
        )

        #expect(track.positionText == "A")
    }
}
