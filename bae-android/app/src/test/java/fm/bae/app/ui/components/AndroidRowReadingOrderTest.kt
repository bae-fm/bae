package fm.bae.app.ui.components

import android.app.Application
import android.view.View
import androidx.compose.material3.Text
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.shots.ElementList
import fm.bae.app.shots.ReadingOrder
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidRowReadingOrderTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aRowsTrailingButtonIsReadAfterItsTitleAndTheLineUnderIt() {
        compose.setContent {
            TestTheme {
                AndroidRow(title = "Role", subtitle = "Member") {
                    AndroidButton(kind = ButtonKind.TEXT, onClick = {}) { Text("Make admin") }
                }
            }
        }
        val root = compose.onRoot().fetchSemanticsNode()
        ReadingOrder.enableTalkBack(root)
        compose.waitForIdle()
        val elements =
            ElementList
                .of(
                    "android.test.row-order",
                    root,
                    unmerged = compose.onRoot(useUnmergedTree = true).fetchSemanticsNode(),
                    reading = ReadingOrder.of(root.root as View),
                ).getJSONArray("elements")
        val labels =
            (0 until elements.length())
                .map {
                    elements.getJSONObject(
                        it,
                    )
                }.filterNot { it.isNull("label") }
                .map { it.getString("label") }

        assertEquals(listOf("Role", "Member", "Make admin"), labels)
    }
}
