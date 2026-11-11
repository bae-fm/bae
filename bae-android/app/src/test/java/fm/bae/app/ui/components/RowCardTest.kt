package fm.bae.app.ui.components

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertHeightIsEqualTo
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.BaeTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/** A [RowCard] holds for any content, including none: it hides the first row's top hairline without going below 0 high. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class RowCardTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aCardWithNoRowsMeasuresZeroHigh() {
        compose.setContent { BaeTheme { RowCard(modifier = Modifier.testTag("card")) {} } }

        compose.onNodeWithTag("card").assertHeightIsEqualTo(0.dp)
    }
}
