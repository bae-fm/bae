import Testing

@testable import BaeKit

@Suite("Playback anchor")
struct PlaybackAnchorTests {
    private static let second: UInt64 = 1_000_000_000

    private func anchor(
        at positionMs: Int64 = 10_000,
        rate: Double = 1,
        durationMs: UInt64? = 180_000
    ) -> PlaybackAnchor {
        PlaybackAnchor(
            positionMs: positionMs,
            rate: rate,
            anchoredAtNs: 5 * Self.second,
            durationMs: durationMs
        )
    }

    @Test("a frame shows the anchor plus the time since it times the rate")
    func anchorPlusElapsedTimesRate() {
        let line = anchor()
        #expect(line.positionMs(atFrameNs: 5 * Self.second) == 10_000)
        #expect(line.positionMs(atFrameNs: 8 * Self.second) == 13_000)
        #expect(
            line.positionMs(atFrameNs: 5 * Self.second + 16_666_667) == 10_016
        )
        #expect(
            anchor(rate: 0.5).positionMs(atFrameNs: 9 * Self.second) == 12_000
        )
    }

    @Test("a paused anchor stands where it is")
    func pausedStands() {
        let line = anchor(rate: 0)
        #expect(line.positionMs(atFrameNs: 500 * Self.second) == 10_000)
        #expect(line.progress(atFrameNs: 500 * Self.second) == 10.0 / 180.0)
    }

    @Test("the position is held to the track's length")
    func clampedToLength() {
        let line = anchor(at: 170_000)
        #expect(line.positionMs(atFrameNs: 10 * Self.second) == 175_000)
        #expect(line.positionMs(atFrameNs: 50 * Self.second) == 180_000)
        #expect(line.progress(atFrameNs: 50 * Self.second) == 1)
    }

    @Test("a frame timed before the anchor shows the anchor")
    func frameBeforeAnchor() {
        #expect(anchor().positionMs(atFrameNs: 2 * Self.second) == 10_000)
    }

    @Test("the pregap runs up from below zero")
    func pregap() {
        let line = anchor(at: -2_000)
        #expect(line.positionMs(atFrameNs: 6 * Self.second) == -1_000)
        #expect(line.progress(atFrameNs: 6 * Self.second) == 0)
        #expect(line.positionMs(atFrameNs: 8 * Self.second) == 1_000)
    }

    @Test("a track of unknown length is never filled and never held")
    func unknownLength() {
        let line = anchor(durationMs: nil)
        #expect(line.positionMs(atFrameNs: 65 * Self.second) == 70_000)
        #expect(line.progress(atFrameNs: 65 * Self.second) == 0)
    }

    @Test("the stream's value is the anchor a view draws from")
    func streamValueIsTheAnchor() {
        let position = BridgeTrackPosition(
            positionMs: 45_000,
            rate: 1,
            anchoredAtNs: 5 * Self.second,
            durationMs: 180_000,
            seekRevision: 3
        )
        #expect(
            position.anchor
                == PlaybackAnchor(
                    positionMs: 45_000,
                    rate: 1,
                    anchoredAtNs: 5 * Self.second,
                    durationMs: 180_000
                )
        )
    }
}
