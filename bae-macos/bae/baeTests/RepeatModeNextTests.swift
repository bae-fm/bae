import BaeKit
import Testing

/// Pins the UI-owned repeat cycle order: off → context → track → off. Core
/// only accepts absolute `setRepeatMode` values; each platform computes the
/// next mode from what it renders.
@Suite("BridgeRepeatMode.next")
struct RepeatModeNextTests {
    @Test("off advances to context")
    func offToContext() {
        #expect(BridgeRepeatMode.off.next == .context)
    }

    @Test("context advances to track")
    func contextToTrack() {
        #expect(BridgeRepeatMode.context.next == .track)
    }

    @Test("track wraps back to off")
    func trackToOff() {
        #expect(BridgeRepeatMode.track.next == .off)
    }
}
