package fm.bae.app

import android.content.Context
import fm.bae.app.ui.settings.RestoreCodePage
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.util.Locale
import java.util.TimeZone

/** Numbers, dates and times show in the app's locale: its digits, grouping and clock. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class NumberTextTest {
    private val context: Context = RuntimeEnvironment.getApplication()

    @Test
    fun countsTakeTheLocalesDigitsAndGrouping() {
        assertEquals("1,234", integerText(1234, Locale.forLanguageTag("en-US")))
        assertEquals("1 234", integerText(1234, Locale.forLanguageTag("fr-FR")))
        assertEquals("١٬٢٣٤", integerText(1234, Locale.forLanguageTag("ar-EG")))
    }

    @Test
    fun aYearHasNoGrouping() {
        assertEquals("1999", integerText(1999, Locale.forLanguageTag("en-US"), grouping = false))
        assertEquals("١٩٩٩", integerText(1999, Locale.forLanguageTag("ar-EG"), grouping = false))
    }

    @Test
    @Config(qualifiers = "ar-rEG")
    fun countTextFollowsTheConfigurationsLocale() {
        assertEquals("٣", context.countText(3))
        assertEquals("٣", context.countText(3u))
        assertEquals("١٩٩٩", context.yearText(1999))
    }

    @Test
    fun aRetryTimeUsesTheLocalesClock() {
        val at = 1_700_000_000_000L // 2023-11-14 22:13:20 UTC
        val utc = TimeZone.getTimeZone("UTC")
        assertEquals("10:13 PM", plainSpaces(retryTimeText(at, Locale.forLanguageTag("en-US"), utc)))
        assertEquals("22:13", retryTimeText(at, Locale.forLanguageTag("fr-FR"), utc))
    }

    /**
     * A clock's space before PM is U+202F in a JDK whose locale data is CLDR 42 or newer (JDK 20 and
     * later) and a plain space in JDK 17's; the test is of the 12-hour clock, not of that space.
     */
    private fun plainSpaces(text: String): String = text.replace('\u202f', ' ').replace('\u00a0', ' ')

    @Test
    @Config(qualifiers = "fr")
    fun aPrintedRestoreCodeDatesInTheConfigurationsLocale() {
        val page = RestoreCodePage.of(context, "code", "place", java.util.Date(1_700_000_000_000L))
        assertEquals(true, page.printed.contains("novembre"))
    }
}
