package fm.bae.app.ui.library

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import fm.bae.app.BaeRobolectricTestRunner
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeDay
import java.time.LocalDate
import java.util.TimeZone

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class HistoryWordsTest {
    private val context = ApplicationProvider.getApplicationContext<Application>()
    private val today = LocalDate.of(2026, 11, 19)

    @Test
    fun theDayOfToday() {
        assertEquals("Today", dayHeading(context, BridgeDay(year = 2026, month = 11u, day = 19u), today))
    }

    @Test
    fun theDayBeforeToday() {
        assertEquals("Yesterday", dayHeading(context, BridgeDay(year = 2026, month = 11u, day = 18u), today))
    }

    @Test
    fun anOlderDayNamesItsWeekdayAndDate() {
        assertEquals(
            "Monday, November 16, 2026",
            dayHeading(context, BridgeDay(year = 2026, month = 11u, day = 16u), today),
        )
    }

    @Test
    fun aDayAcrossTheNewYearIsYesterday() {
        assertEquals(
            "Yesterday",
            dayHeading(context, BridgeDay(year = 2026, month = 12u, day = 31u), LocalDate.of(2027, 1, 1)),
        )
    }

    @Test
    fun aPlaysTimeShowsInTheZoneGiven() {
        val startedAt = 1_795_124_460_000L
        assertEquals("9:41 PM", timeOf(context, startedAt, TimeZone.getTimeZone("UTC")))
        assertEquals("4:41 PM", timeOf(context, startedAt, TimeZone.getTimeZone("America/New_York")))
    }
}
