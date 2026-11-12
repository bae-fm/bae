package fm.bae.app.shots

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import fm.bae.app.BaeRobolectricTestRunner
import org.junit.Assume.assumeNotNull
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.rules.TestName
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * A board's shot: the real composable drawn at a current phone size in the boards' look (dark, Midnight,
 * Pink), to `<board>@android.png` and its element list `<board>@android.json` in the directory
 * `-Pbae.shotsDir` names (bae-shots passes it). Without it the shot is skipped, and so is a board no
 * prefix of `-Pbae.shotsSelect` (comma-separated board-name prefixes) starts, when that is given.
 *
 * A shot is a test function named by its board in PascalCase (`AndroidWelcomeLibraries` is
 * `android.welcome.libraries`), written as camelCase; [shot] refuses a board its function doesn't spell.
 */
@org.junit.runner.RunWith(BaeRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], application = Application::class, qualifiers = PHONE_QUALIFIERS)
abstract class BoardShot {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @get:Rule
    val test = TestName()

    /**
     * Draws a design-system component in one of its states, as [id] (`android.component.<thing>.<state>`) names
     * it, on the page colour. A [wide] state is drawn as wide as the phone, as the preview's wide rows are.
     */
    protected fun component(
        id: String,
        wide: Boolean = false,
        content: @Composable () -> Unit,
    ) = shot(id) { ComponentScene(wide, content) }

    /** Draws [content] as [board]. */
    protected fun shot(
        board: String,
        content: @Composable () -> Unit,
    ) {
        val spelled = board.split('.', '-').joinToString("") { it.replaceFirstChar(Char::uppercase) }
        require(spelled.replaceFirstChar(Char::lowercase) == test.methodName) {
            "shot '${test.methodName}' is not the camelCase of board '$board' ($spelled)"
        }
        ComponentIds.check(board)
        val directory = System.getProperty("bae.shotsDir")
        assumeNotNull(directory)
        assumeTrue("board '$board' is not selected", isSelected(board, System.getProperty("bae.shotsSelect")))
        compose.settle(content)
        val root = compose.onRoot()
        root.captureRoboImage(File(directory, "$board@android.png").path)
        File(directory, "$board@android.json")
            .writeText(ElementList.of(board, root.fetchSemanticsNode()).toString(JSON_INDENT) + "\n")
    }
}

/** Whether [board] is in the run: every board when [select] is absent, else those a listed prefix starts. */
internal fun isSelected(
    board: String,
    select: String?,
): Boolean = select.isNullOrEmpty() || select.split(',').any { board.startsWith(it) }

/**
 * Draws [content] and stops at the same frame every run: the clock advances only by [SETTLE_MILLIS],
 * so a spinner, the welcome logo's bob and glow, or a progress bar is caught at one moment, and an
 * animation that never ends can't keep the test waiting for idle. The content is in inspection mode,
 * as a preview is: the person's files aren't read.
 */
internal fun ComposeContentTestRule.settle(content: @Composable () -> Unit) {
    mainClock.autoAdvance = false
    setContent { CompositionLocalProvider(LocalInspectionMode provides true, content = content) }
    mainClock.advanceTimeBy(SETTLE_MILLIS)
}

/** 411 by 914 dp at 420 dpi (2.625 pixels to the dp), dark. */
internal const val PHONE_QUALIFIERS = "w411dp-h914dp-night-420dpi"
private const val SETTLE_MILLIS = 100L
private const val JSON_INDENT = 2
