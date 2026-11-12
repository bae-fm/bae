package fm.bae.app

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * The size rule the boards print: decimal units, at most one decimal, none
 * when the rounded number is whole, the number and the unit's name in the
 * person's language.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "en-rUS")
class FileSizeTextTest {
    private val context: Context = ApplicationProvider.getApplicationContext()

    private fun size(bytes: Long) = context.formatFileSize(bytes)

    @Test
    fun printsTheBoardsSizes() {
        assertEquals("3.9 GB", size(3_860_000_000))
        assertEquals("1.2 GB", size(1_200_000_000))
        assertEquals("420 MB", size(420_000_000))
        assertEquals("1 GB", size(1_000_000_000))
        assertEquals("20 GB", size(20_000_000_000))
        assertEquals("6.4 MB", size(6_400_000))
        assertEquals("6 KB", size(6_000))
    }

    @Test
    fun smallCounts() {
        assertEquals("0 bytes", size(0))
        assertEquals("999 bytes", size(999))
        assertEquals("1 byte", size(1))
        assertEquals("1 KB", size(1_000))
    }

    @Test
    fun aValueThatRoundsToAThousandMovesUpAUnit() {
        assertEquals("999.9 KB", size(999_900))
        assertEquals("1 MB", size(999_960))
        assertEquals("1 TB", size(999_960_000_000))
    }

    @Test
    fun largestUnitIsTerabytes() {
        assertEquals("12,000 TB", size(12_000_000_000_000_000))
    }

    @Test
    @Config(qualifiers = "fr-rFR")
    fun frenchUsesItsSeparatorAndUnitNames() {
        assertEquals("3,9 Go", size(3_860_000_000).replace(' ', ' ').replace(' ', ' '))
        assertEquals("420 Mo", size(420_000_000).replace(' ', ' ').replace(' ', ' '))
    }

    @Test
    @Config(qualifiers = "de-rDE")
    fun germanUsesItsSeparator() {
        assertEquals("3,9 GB", size(3_860_000_000).replace(' ', ' ').replace(' ', ' '))
    }
}
