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

/** A [AndroidCard] holds for any content, including none: it measures 0 high with no rows, with no gap left over. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidCardTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aCardWithNoRowsMeasuresZeroHigh() {
        compose.setContent { BaeTheme { AndroidCard(modifier = Modifier.testTag("card")) {} } }

        compose.onNodeWithTag("card").assertHeightIsEqualTo(0.dp)
    }
}
