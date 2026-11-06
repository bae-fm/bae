package fm.bae.app.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class CountdownTest {
    @Test
    fun secondsLeftRoundUpAndStopAtZero() {
        assertEquals(5L, countdownSecondsLeft(deadlineMs = 5_000L, nowMs = 0L))
        assertEquals(5L, countdownSecondsLeft(deadlineMs = 5_000L, nowMs = 999L))
        assertEquals(4L, countdownSecondsLeft(deadlineMs = 5_000L, nowMs = 1_000L))
        assertEquals(1L, countdownSecondsLeft(deadlineMs = 5_000L, nowMs = 4_999L))
        assertEquals(0L, countdownSecondsLeft(deadlineMs = 5_000L, nowMs = 5_000L))
        assertEquals(0L, countdownSecondsLeft(deadlineMs = 5_000L, nowMs = 9_000L))
    }
}
