package fm.bae.app.ui

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeRepeatMode

/**
 * Pins the UI-owned repeat cycle order: OFF → CONTEXT → TRACK → OFF. Core only
 * accepts absolute setRepeatMode values; the caller computes the next mode from
 * what it renders.
 */
class RepeatModeCycleTest {
    @Test
    fun offAdvancesToContext() {
        assertEquals(BridgeRepeatMode.CONTEXT, BridgeRepeatMode.OFF.next())
    }

    @Test
    fun contextAdvancesToTrack() {
        assertEquals(BridgeRepeatMode.TRACK, BridgeRepeatMode.CONTEXT.next())
    }

    @Test
    fun trackWrapsToOff() {
        assertEquals(BridgeRepeatMode.OFF, BridgeRepeatMode.TRACK.next())
    }
}
