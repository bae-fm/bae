import BaeKit
import Testing

/// What a play shows: nothing when it played everything it named, and core's
/// line otherwise, with how many it left out.
struct PlayOutcomeLineTests {
    @Test
    func aPlayThatPlayedEverythingShowsNothing() {
        #expect(BridgePlayOutcome.playing.shown == nil)
    }

    @Test
    func aStartTrackThatIsGoneSaysNothingPlayed() {
        #expect(
            BridgePlayOutcome.startMissing.shown?.line
                == "That track is no longer there, so nothing played."
        )
    }

    @Test
    func whatAPlayLeftOutIsCounted() {
        #expect(
            BridgePlayOutcome.playingWithoutTracks(count: 2)
                .shown?
                .line
                == "2 tracks are not in the library and were left out"
        )
        #expect(
            BridgePlayOutcome.playingWithoutReleases(count: 1)
                .shown?
                .line
                == "1 album has no tracks and was left out"
        )
    }
}
