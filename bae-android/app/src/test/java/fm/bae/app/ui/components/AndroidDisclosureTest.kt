package fm.bae.app.ui.components

import android.app.Application
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertWidthIsEqualTo
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.TestTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidDisclosureTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun theWholeRowIsTheButtonNotJustItsWords() {
        // The test reads layout: the clock stops and advances by the same amount on every run.
        compose.mainClock.autoAdvance = false
        compose.setContent {
            TestTheme {
                AndroidDisclosure(label = "Removed devices (2)", open = false, onToggle = {}) {
                    Text("A row", modifier = Modifier.fillMaxWidth())
                }
            }
        }
        compose.mainClock.advanceTimeBy(CLOCK_STEP_MS)
        val bounds = compose.onRoot().getUnclippedBoundsInRoot()
        val width = bounds.right - bounds.left
        compose.onNodeWithText("Removed devices (2)").assertWidthIsEqualTo(width)
    }
}

private const val CLOCK_STEP_MS = 1_000L
