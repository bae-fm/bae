package fm.bae.app.ui.components

import android.app.Application
import androidx.compose.material3.Text
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.junit4.v2.createComposeRule
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.TestTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidSheetPaneTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aSheetWithATitleNamesItsWindowByThatTitle() {
        compose.setContent {
            TestTheme { AndroidSheet(onDismiss = {}, heading = AndroidSheetHeading(title = "Play on")) { Text("Living Room") } }
        }

        compose.onNode(SemanticsMatcher.expectValue(SemanticsProperties.PaneTitle, "Play on")).assertExists()
    }
}
