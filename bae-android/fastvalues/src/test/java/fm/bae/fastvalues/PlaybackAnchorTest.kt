package fm.bae.fastvalues

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeTrackPosition

class PlaybackAnchorTest {
    /** An anchor taken at 5 s on the frame clock. */
    private fun anchor(
        positionMs: Long = 10_000,
        rate: Double = 1.0,
        durationMs: ULong? = 180_000uL,
    ) = BridgeTrackPosition(positionMs, rate, 5uL * SECOND.toULong(), durationMs)

    @Test
    fun aFrameShowsTheAnchorPlusTheTimeSinceItTimesTheRate() {
        val line = anchor()
        assertEquals(10_000L, line.positionAt(5 * SECOND))
        assertEquals(13_000L, line.positionAt(8 * SECOND))
        assertEquals(10_016L, line.positionAt(5 * SECOND + 16_666_667))
        assertEquals(12_000L, anchor(rate = 0.5).positionAt(9 * SECOND))
    }

    @Test
    fun aPausedAnchorStandsWhereItIs() {
        val line = anchor(rate = 0.0)
        assertEquals(10_000L, line.positionAt(500 * SECOND))
        assertEquals(10.0 / 180.0, line.progressAt(500 * SECOND), 1e-12)
    }

    @Test
    fun thePositionIsHeldToTheTracksLength() {
        val line = anchor(positionMs = 170_000)
        assertEquals(175_000L, line.positionAt(10 * SECOND))
        assertEquals(180_000L, line.positionAt(50 * SECOND))
        assertEquals(1.0, line.progressAt(50 * SECOND), 0.0)
    }

    @Test
    fun aFrameTimedBeforeTheAnchorShowsTheAnchor() {
        assertEquals(10_000L, anchor().positionAt(2 * SECOND))
    }

    @Test
    fun thePregapRunsUpFromBelowZero() {
        val line = anchor(positionMs = -2_000)
        assertEquals(-1_000L, line.positionAt(6 * SECOND))
        assertEquals(0.0, line.progressAt(6 * SECOND), 0.0)
        assertEquals(1_000L, line.positionAt(8 * SECOND))
    }

    @Test
    fun aTrackOfUnknownLengthIsNeverFilledAndNeverHeld() {
        val line = anchor(durationMs = null)
        assertEquals(70_000L, line.positionAt(65 * SECOND))
        assertEquals(0.0, line.progressAt(65 * SECOND), 0.0)
    }

    private companion object {
        const val SECOND = 1_000_000_000L
    }
}
