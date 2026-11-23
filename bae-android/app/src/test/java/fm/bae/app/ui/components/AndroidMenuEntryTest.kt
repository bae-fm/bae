package fm.bae.app.ui.components

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/** A menu is as wide as its longest item: a row of a short word is not held to Material's 112 dp least width. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidMenuEntryTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aRowOfAShortWordIsNarrowerThanMaterialsLeastWidth() {
        compose.setContent {
            TestTheme {
                AndroidMenuEntry(
                    text = { Text("Go") },
                    onClick = {},
                    modifier = Modifier.testTag("row").width(IntrinsicSize.Max),
                )
            }
        }
        val width = compose.onNodeWithTag("row").getUnclippedBoundsInRoot().width
        assertTrue("the row is $width wide", width < 112.dp)
    }
}
