package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class DisplayErrorTest {
    /** The excerpt keeps the whole diagnostic when it is short. */
    @Test
    fun aShortDiagnosticIsTheExcerptWhole() {
        val shown = DisplayError("Couldn't reach the network.", "  dns error: no such host  \nat the second line")

        assertEquals("  dns error: no such host  \nat the second line", shown.detailExcerpt)
    }

    @Test
    fun aLongDiagnosticIsCutInTheExcerpt() {
        val shown = DisplayError("Something went wrong.", "x".repeat(500))

        assertEquals("x".repeat(400) + "…", shown.detailExcerpt)
    }

    /** An error this app words itself has no diagnostic to open. */
    @Test
    fun noDiagnosticNamesNoFault() {
        assertNull(DisplayError("Couldn't open the library.").detailExcerpt)
    }
}
