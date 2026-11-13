package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import java.util.Locale

/**
 * The wait an AirPlay receiver asks for, as the length of time its line
 * names: core sends the seconds, and this words them for the locale.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class WaitTextTest {
    private val en = Locale.forLanguageTag("en-US")

    @Test
    fun aWaitUnderAMinuteIsInSeconds() {
        assertEquals("30 seconds", waitText(30, en))
    }

    @Test
    fun aLongerWaitNamesEachUnitItHas() {
        assertEquals("1 minute, 30 seconds", waitText(90, en))
        assertEquals("1 hour, 1 second", waitText(3601, en))
    }

    @Test
    fun noWaitIsZeroSeconds() {
        assertEquals("0 seconds", waitText(0, en))
    }
}
