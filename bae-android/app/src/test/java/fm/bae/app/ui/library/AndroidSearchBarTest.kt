package fm.bae.app.ui.library

import android.app.Application
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.shots.settleInBaeLook
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidSearchBarTest {
    @get:Rule
    val compose = createComposeRule()

    /** The field's own box is the whole 56 dp pill, as the board's field is, not the text inside it. */
    @Test
    fun theFieldIsThePill() {
        compose.settleInBaeLook { TestTheme { AndroidSearchBar(query = "", onQueryChange = {}, onClose = {}) } }

        val box = compose.onNode(hasSetTextAction()).fetchSemanticsNode().boundsInRoot
        with(compose.density) { assertEquals(56.dp.toPx(), box.height, 1f) }
    }
}
