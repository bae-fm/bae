package fm.bae.app.ui.components

import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Test

/** The usage bar gives every kind at least 4dp and shares the rest by proportion. */
class StorageUsageWidthsTest {
    @Test
    fun aThinKindIsRaisedToTheLeastAndTheOthersGiveItUp() {
        val widths = segmentWidths(listOf(0.001f, 0.5f, 0.499f), 204.dp)

        assertEquals(4f, widths[0].value, 0.001f)
        assertEquals(100f, widths[1].value, 0.5f)
        assertEquals(204f, widths.sumOf { it.value.toDouble() }.toFloat(), 0.001f)
    }

    @Test
    fun kindsAllWideEnoughKeepTheirShares() {
        val widths = segmentWidths(listOf(0.25f, 0.75f), 200.dp)

        assertEquals(50f, widths[0].value, 0.001f)
        assertEquals(150f, widths[1].value, 0.001f)
    }
}
