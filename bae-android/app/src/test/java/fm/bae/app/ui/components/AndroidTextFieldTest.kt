package fm.bae.app.ui.components

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Column
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * The helper under an [AndroidTextField] is a text of its own, not part of the field's box, so the field itself carries
 * the helper for TalkBack: as its error when it is in error, else as its state description.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidTextFieldTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private fun show(
        helper: String?,
        error: Boolean,
    ) {
        compose.setContent {
            TestTheme {
                AndroidTextField(
                    value = "",
                    onValueChange = {},
                    label = "Name",
                    modifier = Modifier.testTag("field"),
                    helper = helper,
                    error = error,
                )
            }
        }
    }

    @Test
    fun aFieldCarriesItsHelperAsItsStateDescription() {
        show(helper = "Everyone sees this name.", error = false)

        val config = compose.onNodeWithTag("field").fetchSemanticsNode().config
        assertEquals("Everyone sees this name.", config[SemanticsProperties.StateDescription])
        assertNull(config.getOrNull(SemanticsProperties.Error))
    }

    @Test
    fun aFieldInErrorCarriesItsHelperAsItsError() {
        show(helper = "Enter a name.", error = true)

        val config = compose.onNodeWithTag("field").fetchSemanticsNode().config
        assertEquals("Enter a name.", config[SemanticsProperties.Error])
    }

    @Test
    fun aFieldWithoutAHelperCarriesNone() {
        show(helper = null, error = false)

        val config = compose.onNodeWithTag("field").fetchSemanticsNode().config
        assertNull(config.getOrNull(SemanticsProperties.StateDescription))
        assertNull(config.getOrNull(SemanticsProperties.Error))
    }

    @Test
    fun aFieldIsAsHighWithAHelperAsWithout() {
        // The outlined field's label animates: the clock stops and advances by the same amount on every run.
        compose.mainClock.autoAdvance = false
        compose.setContent {
            TestTheme {
                Column {
                    AndroidTextField(
                        value = "",
                        onValueChange = {},
                        label = "Name",
                        modifier = Modifier.testTag("bare"),
                    )
                    AndroidTextField(
                        value = "",
                        onValueChange = {},
                        label = "Name",
                        modifier = Modifier.testTag("helped"),
                        helper = "Everyone sees this name.",
                    )
                }
            }
        }

        compose.mainClock.advanceTimeBy(CLOCK_STEP_MILLIS)
        val bare =
            compose
                .onNodeWithTag("bare")
                .fetchSemanticsNode()
                .size.height
        val helped =
            compose
                .onNodeWithTag("helped")
                .fetchSemanticsNode()
                .size.height
        assertEquals(bare, helped)
    }
}

private const val CLOCK_STEP_MILLIS = 500L
