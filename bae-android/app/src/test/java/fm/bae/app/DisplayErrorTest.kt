package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class DisplayErrorTest {
    /** The summary is the diagnostic's first line; the excerpt and the copy keep the rest. */
    @Test
    fun theSummaryIsTheFirstLineOfTheDiagnostic() {
        val shown = DisplayError("Couldn't reach the network.", "  dns error: no such host  \nat the second line")

        assertEquals("dns error: no such host", shown.detailSummary)
        assertEquals("  dns error: no such host  \nat the second line", shown.detailExcerpt)
    }

    @Test
    fun aLongDiagnosticIsCutInTheSummaryAndTheExcerpt() {
        val shown = DisplayError("Something went wrong.", "x".repeat(500))

        assertEquals("x".repeat(180) + "…", shown.detailSummary)
        assertEquals("x".repeat(400) + "…", shown.detailExcerpt)
    }

    /** An error this app words itself, or a blank diagnostic, has no fault to name. */
    @Test
    fun noDiagnosticNamesNoFault() {
        assertNull(DisplayError("Couldn't open the library.").detailSummary)
        assertNull(DisplayError("Couldn't open the library.").detailExcerpt)
        assertNull(DisplayError("Something went wrong.", "   \nsecond").detailSummary)
    }
}
