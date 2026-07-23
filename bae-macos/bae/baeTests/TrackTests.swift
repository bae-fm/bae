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
                side: 1,
                trackNumber: nil,
                durationMs: nil,
                durationClock: nil,
                artistNames: "Artist Name",
                displayArtist: nil,
                positionText: "A"
            )
        )

        #expect(track.positionText == "A")
    }
}
