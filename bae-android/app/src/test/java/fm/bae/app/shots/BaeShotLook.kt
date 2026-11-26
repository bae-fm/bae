package fm.bae.app.shots

import android.graphics.Rect
import android.view.View
import android.widget.SeekBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import builds.shots.ShotLook
import builds.shots.settle
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.ui.LocalNowMs
import fm.bae.app.ui.playback.SeekLineDrawable
import kotlin.math.roundToInt

/** How bae draws its views in a shot. */
internal object BaeShotLook : ShotLook {
    @Composable
    override fun Dress(content: @Composable () -> Unit) {
        CompositionLocalProvider(
            LocalNowMs provides ShotClock::nowMs,
            LocalBugReports provides BugReports(),
            content = content,
        )
    }

    @Composable
    override fun ComponentPage(content: @Composable () -> Unit) {
        ShotTheme {
            Surface(color = MaterialTheme.colorScheme.background, content = content)
        }
    }

    /** A seek line's box is the line the board draws as the bar's track, centred in the view's touch height and inside its thumb padding. */
    override fun drawnBox(view: View): Rect {
        val line = (view as? SeekBar)?.progressDrawable as? SeekLineDrawable ?: return super.drawnBox(view)
        val height = (line.look.lineHeightDp * view.resources.displayMetrics.density).roundToInt()
        val top = (view.height - height) / 2
        return Rect(view.paddingStart, top, view.width - view.paddingEnd, top + height)
    }
}

/**
 * [content] drawn as a shot draws it, stopped at the same frame every run ([settle]). The content stages its own
 * theme through the appearance store it gives [fm.bae.app.ui.BaeTheme], as [ShotTheme] does.
 */
internal fun ComposeContentTestRule.settleInBaeLook(content: @Composable () -> Unit) = settle(BaeShotLook, content)
