package fm.bae.app.ui.components

import fm.bae.app.DisplayError
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ErrorDetailsTest {
    @Test
    fun aLineWithNoDetailHasNoDetails() {
        assertNull(ErrorDetails.of(DisplayError("Couldn't open the library.")))
    }

    @Test
    fun aLineWithADetailOpensWithTheLineAsTitleAndTheWholeDetailToCopy() {
        val detail = "x".repeat(500)

        val details = ErrorDetails.of(DisplayError("Couldn't reach the network.", detail))!!

        assertEquals("Couldn't reach the network.", details.title)
        assertNull(details.message)
        assertEquals("x".repeat(400) + "…", details.causes)
        assertEquals(detail, details.copyText)
    }

    @Test
    fun aTitledNoticeLeadsItsCausesAndCopyWithTheLine() {
        val details =
            ErrorDetails.titled("Couldn't read the sign-in settings", "Reinstall.", DisplayError("Missing.", "client id"))

        assertEquals("Couldn't read the sign-in settings", details.title)
        assertEquals("Reinstall.", details.message)
        assertEquals("Missing.\nclient id", details.causes)
        assertEquals("Missing.\nclient id", details.copyText)
    }

    @Test
    fun aTitledNoticeWithNoDetailHasTheLineAlone() {
        val details = ErrorDetails.titled("Could not remove the library", null, DisplayError("Disk full."))

        assertEquals("Disk full.", details.causes)
        assertEquals("Disk full.", details.copyText)
    }
}
