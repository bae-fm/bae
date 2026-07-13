package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeDurationClock
import java.util.Locale

/**
 * The clock renderer's half of the contract: core decides which fields a label
 * has (bae-core's `util::duration` tests pin that), and this turns those fields
 * into digits. Nothing is shown for a clock core declined to produce.
 */
class ClockTextTest {
    private val en = Locale.forLanguageTag("en-US")

    private fun clock(
        negative: Boolean = false,
        hours: ULong? = null,
        minutes: UInt,
        seconds: UInt,
    ) = BridgeDurationClock(negative = negative, hours = hours, minutes = minutes, seconds = seconds)

    @Test
    fun rendersMinutesAndSecondsWithoutAnHoursField() {
        assertEquals("3:07", clockText(clock(minutes = 3u, seconds = 7u), en))
        assertEquals("0:00", clockText(clock(minutes = 0u, seconds = 0u), en))
        assertEquals("59:59", clockText(clock(minutes = 59u, seconds = 59u), en))
    }

    @Test
    fun everyFieldAfterTheFirstIsPaddedToTwoDigits() {
        assertEquals("1:02:03", clockText(clock(hours = 1uL, minutes = 2u, seconds = 3u), en))
        assertEquals("10:00:00", clockText(clock(hours = 10uL, minutes = 0u, seconds = 0u), en))
    }

    @Test
    fun aNegativeClockCountsDownWithALeadingMinus() {
        assertEquals("-2:30", clockText(clock(negative = true, minutes = 2u, seconds = 30u), en))
        assertEquals("-0:00", clockText(clock(negative = true, minutes = 0u, seconds = 0u), en))
    }

    /** No clock means no label — the duration is absent or nonsense. */
    @Test
    fun noClockRendersNothing() {
        assertEquals("", clockText(null, en))
    }

    /** The digits are the locale's; the `:` separators and the padding are not. */
    @Test
    fun digitsFollowTheLocale() {
        assertEquals("٣:٠٧", clockText(clock(minutes = 3u, seconds = 7u), Locale.forLanguageTag("ar-EG")))
        assertEquals("3:07", clockText(clock(minutes = 3u, seconds = 7u), Locale.forLanguageTag("de-DE")))
    }
}
