package fm.bae.app.shots

import androidx.compose.material3.Text
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.AndroidSheetHeading
import org.junit.Assert.assertEquals
import org.junit.Test

/** The element list of a sheet drawn over its screen. */
class SheetSceneTest : BoardShot() {
    @Test
    fun aSheetPinnedBelowTheTopIsNamedAtTheSheetsOwnBox() {
        compose.settle {
            ScreenScene { SheetOverScrim(AndroidSheetHeading("Not on this device"), top = 60.dp) { Text("The sheet’s content") } }
        }
        val elements =
            ElementList
                .of(
                    "android.test.sheet",
                    compose.onRoot().fetchSemanticsNode(),
                    unmerged = compose.onRoot(useUnmergedTree = true).fetchSemanticsNode(),
                ).getJSONArray("elements")
        val named =
            (0 until elements.length()).map { elements.getJSONObject(it) }.single {
                it.optString("label") == "Not on this device" &&
                    it.getString("role") == "other"
            }

        // The sheet starts 60 dp down, not at the top of the cell its scrim fills.
        assertEquals(60.0, named.getJSONObject("bounds").getDouble("y"), 0.5)
    }
}
