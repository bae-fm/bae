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
                durationClock: BridgeDurationClock.length(ms: 180_000),
                artistLine: .named("Artist Name"),
                displayArtist: nil,
                positionText: "A",
                works: [],
                rating: .stars(mine: nil),
            )
        )

        #expect(track.positionText == "A")
    }

    @Test(
        "a track carries the works it performs, in order, and the person's stars"
    )
    func carriesWorksAndStars() {
        let works = [
            BridgeTrackWork(
                workId: "work-1",
                title: "Zeta Work",
                composerNames: "Composer A"
            ),
            BridgeTrackWork(
                workId: "work-2",
                title: "Alpha Work",
                composerNames: nil
            ),
        ]
        let track = Track(
            from: BridgeTrack(
                id: "track-1",
                title: "Track Title",
                durationMs: 180_000,
                durationClock: BridgeDurationClock.length(ms: 180_000),
                artistLine: .named("Artist Name"),
                displayArtist: nil,
                positionText: "1",
                works: works,
                rating: .stars(mine: .four),
            )
        )

        #expect(track.works.map(\.workId) == ["work-1", "work-2"])
        #expect(track.works.map(\.composerNames) == ["Composer A", nil])
        #expect(track.rating == .stars(mine: .four))
    }
}
