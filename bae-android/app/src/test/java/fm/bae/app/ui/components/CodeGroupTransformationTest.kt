package fm.bae.app.ui.components

import androidx.compose.ui.text.AnnotatedString
import org.junit.Assert.assertEquals
import org.junit.Test

/** A code field shows its text in groups of four, whether it was typed in groups or run together. */
class CodeGroupTransformationTest {
    private fun shown(typed: String) = CodeGroupTransformation.filter(AnnotatedString(typed)).text.text

    @Test
    fun runTogetherTextShowsInGroupsOfFour() {
        assertEquals("Home musi c", shown("Home music"))
    }

    @Test
    fun groupedTextKeepsItsGroups() {
        assertEquals("CVR1 REST OREC ODE", shown("CVR1 REST OREC ODE"))
    }

    @Test
    fun dashesAndLineBreaksAreNotShown() {
        assertEquals("CVR1 REST", shown("CVR1-REST\n"))
    }

    @Test
    fun cursorKeepsItsPlaceAmongTheShownCharacters() {
        val result = CodeGroupTransformation.filter(AnnotatedString("CVR1RESTOR"))
        val offsets = result.offsetMapping
        assertEquals(4, offsets.originalToTransformed(4))
        assertEquals(6, offsets.originalToTransformed(5))
        assertEquals(12, offsets.originalToTransformed(10))
        assertEquals(4, offsets.transformedToOriginal(5))
        assertEquals(5, offsets.transformedToOriginal(6))
        assertEquals(10, offsets.transformedToOriginal(12))
    }
}
