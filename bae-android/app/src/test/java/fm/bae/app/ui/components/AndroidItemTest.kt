package fm.bae.app.ui.components

import android.app.Application
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.shots.settle
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidItemTest {
    @get:Rule
    val compose = createComposeRule()

    /** A row's text starts 16 dp after its leading glyph, as Material's list items start it, and the trail 8 dp after the text. */
    @Test
    fun theTextStartsSixteenAfterTheLeadAndTheTrailEightAfterTheText() {
        compose.settle {
            TestTheme {
                Box(Modifier.requiredWidth(300.dp)) {
                    AndroidItem(
                        lead = { Box(Modifier.size(20.dp).testTag("lead")) },
                        trail = { Box(Modifier.size(20.dp).testTag("trail")) },
                    ) { Box(Modifier.fillMaxWidth().height(20.dp).testTag("text")) }
                }
            }
        }

        val lead = compose.onNodeWithTag("lead").fetchSemanticsNode().boundsInRoot
        val text = compose.onNodeWithTag("text").fetchSemanticsNode().boundsInRoot
        val trail = compose.onNodeWithTag("trail").fetchSemanticsNode().boundsInRoot
        with(compose.density) {
            assertEquals(16.dp.toPx(), text.left - lead.right, 0.5f)
            assertEquals(8.dp.toPx(), trail.left - text.right, 0.5f)
        }
    }
}
