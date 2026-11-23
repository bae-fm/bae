package fm.bae.app.shots

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import com.github.takahirom.roborazzi.captureRoboImage
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.ui.LocalNowMs
import org.json.JSONObject
import org.junit.Assume.assumeNotNull
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.rules.TestName
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import kotlin.math.ceil

/**
 * A board's shot: the real composable drawn at a current phone size in the boards' look (dark, Midnight,
 * Pink), to `<board>@android.png` and its element list `<board>@android.json` in the directory
 * `-Pbae.shotsDir` names (`bae-android/shots-capture.sh` passes it). Without it the shot is skipped, and
 * so is a board no prefix of `-Pbae.shotsSelect` (comma-separated board-name prefixes) starts, when that
 * is given.
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
     * it, on the page colour, at the size of its board's preview cell, which builds lists in the file
     * `bae.shotsBoardSizes` names; a selected state with no entry fails its test, by its board's name.
     */
    protected fun component(
        id: String,
        content: @Composable () -> Unit,
    ) {
        assumeNotNull(System.getProperty("bae.shotsDir"))
        assumeTrue("board '$id' is not selected", isSelected(id, System.getProperty("bae.shotsSelect")))
        val size = checkNotNull(BoardSizes.of(id)) { "the board sizes list none for '$id'" }
        sizeWindowTo(size)
        shot(id) { ComponentScene(size, content) }
    }

    /**
     * Gives the activity a window as wide and as tall as [size], the board's preview cell, so a view that fills
     * the cell (a dialog centred in it, a sheet at its foot) sits where its board draws it, even when the cell is
     * wider than the phone.
     */
    private fun sizeWindowTo(size: DpSize) {
        val width = ceil(size.width.value).toInt()
        val height = ceil(size.height.value).toInt()
        // Robolectric swaps the two when the orientation it infers disagrees with them, so the orientation is named.
        val orientation = if (width > height) "land" else "port"
        RuntimeEnvironment.setQualifiers("+w${width}dp-h${height}dp-$orientation")
        compose.activityRule.scenario.recreate()
    }

    /**
     * Draws [content] as [board]. A board whose frame is [height] dp tall instead of the phone's gets a window
     * as tall as the frame, which holds the page the board draws below the phone's fold.
     */
    protected fun shot(
        board: String,
        height: Dp? = null,
        content: @Composable () -> Unit,
    ) {
        val spelled = board.split('.', '-').joinToString("") { it.replaceFirstChar(Char::uppercase) }
        require(spelled.replaceFirstChar(Char::lowercase) == test.methodName) {
            "shot '${test.methodName}' is not the camelCase of board '$board' ($spelled)"
        }
        val directory = System.getProperty("bae.shotsDir")
        assumeNotNull(directory)
        assumeTrue("board '$board' is not selected", isSelected(board, System.getProperty("bae.shotsSelect")))
        height?.let { sizeWindowTo(DpSize(PHONE_WIDTH, it)) }
        compose.settle(content)
        val root = compose.onRoot()
        root.captureRoboImage(File(directory, "$board@android.png").path)
        val embedded =
            EmbeddedViews.of(compose.activity.window.decorView, compose.activity.resources.displayMetrics.density)
        val unmerged = compose.onRoot(useUnmergedTree = true).fetchSemanticsNode()
        File(directory, "$board@android.json")
            .writeText(ElementList.of(board, root.fetchSemanticsNode(), embedded, unmerged).toString(JSON_INDENT) + "\n")
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
 * animation that never ends can't keep the test waiting for idle. The content stages its own look
 * through the appearance store it gives [fm.bae.app.ui.BaeTheme], as [ShotTheme] does.
 */
internal fun ComposeContentTestRule.settle(content: @Composable () -> Unit) {
    mainClock.autoAdvance = false
    setContent {
        CompositionLocalProvider(
            LocalNowMs provides ShotClock::nowMs,
            LocalBugReports provides BugReports(),
            content = content,
        )
    }
    mainClock.advanceTimeBy(SETTLE_MILLIS)
}

/** 412 by 916 dp at 420 dpi (2.625 pixels to the dp), dark. */
private val PHONE_WIDTH = 412.dp

internal const val PHONE_QUALIFIERS = "w412dp-h916dp-night-420dpi"
private const val SETTLE_MILLIS = 100L
private const val JSON_INDENT = 2

/** The size of each component board in dp, from the JSON file `bae.shotsBoardSizes` names: `{"<board>": {"width", "height"}}`. */
internal object BoardSizes {
    private val sizes: JSONObject by lazy {
        val path = checkNotNull(System.getProperty("bae.shotsBoardSizes")) { "builds shots gives no board sizes file" }
        JSONObject(File(path).readText())
    }

    fun of(board: String): DpSize? {
        val entry = sizes.optJSONObject(board) ?: return null
        return DpSize(entry.getDouble("width").dp, entry.getDouble("height").dp)
    }
}
