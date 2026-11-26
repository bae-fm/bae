package fm.bae.app.shots

import androidx.compose.ui.test.onRoot
import builds.shots.ElementList
import builds.shots.ReadingOrder
import fm.bae.app.ui.TestTheme
import fm.bae.app.ui.components.AndroidPinField
import fm.bae.app.ui.components.AndroidTextField
import fm.bae.app.ui.library.AndroidSearchBar
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The element list of bae's text field, search field and PIN field. */
class FieldElementListTest : BaeBoardShot() {
    @Test
    fun listsAFieldsLabelAsATextBeforeTheFieldAndTheFieldAsItsOutline() {
        compose.setContent { AndroidTextField(value = "Home music", onValueChange = {}, label = "Name") }
        val elements = elementsOf("android.test.field")

        val label = elements.single { it.getString("role") == "text" }
        val field = elements.single { it.getString("role") == "field" }
        assertEquals("Name", label.getString("label"))
        assertTrue(elements.indexOf(label) < elements.indexOf(field))
        assertEquals(56.0, field.getJSONObject("bounds").getDouble("height"), 0.0)
        assertEquals(8.0, field.getJSONObject("bounds").getDouble("y"), 0.0)
    }

    @Test
    fun recordsTheFullHeightOfTheSearchFieldWhichHasNoOutlineLabel() {
        compose.settleInBaeLook { TestTheme { AndroidSearchBar(query = "", onQueryChange = {}, onClose = {}) } }
        val elements = elementsOf("android.test.search")

        val bounds = elements.single { it.getString("role") == "field" }.getJSONObject("bounds")
        assertEquals(56.0, bounds.getDouble("height"), 0.0)
        assertEquals(8.0, bounds.getDouble("y"), 0.0)
    }

    @Test
    fun recordsTheFullHeightOfAPinFieldWithoutAnOutlineLabel() {
        compose.settleInBaeLook { AndroidPinField(digits = 4, value = "0482", onValueChange = {}) }
        val elements = elementsOf("android.test.pin")

        val bounds = elements.single { it.getString("role") == "field" }.getJSONObject("bounds")
        assertEquals(56.0, bounds.getDouble("height"), 0.0)
        assertEquals(0.0, bounds.getDouble("y"), 0.0)
    }

    /** The elements the shot of the drawn screen lists, in tree order. */
    private fun elementsOf(board: String): List<JSONObject> {
        val elements =
            ElementList
                .of(
                    board,
                    compose.onRoot().fetchSemanticsNode(),
                    embedded = emptyList(),
                    unmerged = compose.onRoot(useUnmergedTree = true).fetchSemanticsNode(),
                    reading = ReadingOrder { null },
                ).getJSONArray("elements")
        return (0 until elements.length()).map(elements::getJSONObject)
    }
}
