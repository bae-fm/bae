package fm.bae.app.ui.sync

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Canvas
import android.view.animation.LinearInterpolator
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.core.graphics.withClip
import fm.bae.app.ui.appearance.ThemeOpacity

/** The gap between the filled part and the rest of the track, and the stop dot's size: Material 3's own. */
private const val GAP_DP = 4f

/**
 * A bar that moves with no share to show, as the line for syncing has none: the accent over a
 * track of the accent's tint. Screen readers name it [description], the line's words.
 */
@Composable
internal fun MovingBar(
    description: String,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    LinearProgressIndicator(
        modifier = modifier.fillMaxWidth().semantics { contentDescription = description },
        color = accent,
        trackColor = accent.copy(alpha = ThemeOpacity.tintStrong),
    )
}

/** How long the moving bar's piece takes to cross the track. */
private const val MOVING_MILLIS = 1500L

/** How far across the track the moving piece rests before the bar first moves. */
private const val MOVING_REST_PHASE = 0.3f

/** The moving bar's piece, as a share of the track's width. */
private const val MOVING_SHARE = 0.4f

/**
 * Material 3's linear progress bar drawn from the fraction [showFraction] hands it: four tall, rounded,
 * the filled part, a gap, the rest of the track and a dot at its end that marks where the bar stops.
 * [showMoving] swaps the fraction for a piece that crosses the track again and again, for a step that
 * counts nothing.
 */
internal class ShareBarView(
    context: Context,
) : TrackBarView(context) {
    private var fraction = 0f
    override val shareDone: Float get() = fraction

    /** Where the moving piece is; before it first moves it sits part way, so a still frame shows it. */
    private var phase = MOVING_REST_PHASE
    private val crossing =
        ValueAnimator.ofFloat(0f, 1f).apply {
            duration = MOVING_MILLIS
            repeatCount = ValueAnimator.INFINITE
            interpolator = LinearInterpolator()
            addUpdateListener {
                phase = it.animatedValue as Float
                invalidate()
            }
        }

    /** Shows [share] of the track filled, without following a stream. */
    fun showFraction(share: Float) {
        crossing.cancel()
        fraction = share
        invalidate()
    }

    /** Shows a piece crossing the track, while a step counts nothing. */
    fun showMoving() {
        if (!crossing.isStarted) crossing.start()
        invalidate()
    }

    override fun onDetachedFromWindow() {
        crossing.cancel()
        super.onDetachedFromWindow()
    }

    override fun onDraw(canvas: Canvas) {
        val height = barHeight()
        val gap = dp(GAP_DP)
        val radius = height / 2f
        if (crossing.isStarted) {
            canvas.drawRoundRect(bounds.apply { set(0f, 0f, width.toFloat(), height) }, radius, radius, track)
            val piece = width * MOVING_SHARE
            val start = -piece + (width + piece) * phase
            canvas.withClip(0f, 0f, width.toFloat(), height) {
                drawRoundRect(bounds.apply { set(start, 0f, start + piece, height) }, radius, radius, fill)
            }
            return
        }
        val filledEnd = width * fraction
        if (fraction > 0f) {
            canvas.drawRoundRect(bounds.apply { set(0f, 0f, filledEnd, height) }, radius, radius, fill)
        }
        val trackStart = if (fraction > 0f) filledEnd + gap else 0f
        if (trackStart < width) {
            canvas.drawRoundRect(bounds.apply { set(trackStart, 0f, width.toFloat(), height) }, radius, radius, track)
        }
        canvas.drawCircle(width - radius, radius, radius, fill)
    }
}
