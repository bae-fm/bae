package fm.bae.app.ui.sync

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.util.TypedValue
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.view.animation.LinearInterpolator
import android.widget.ProgressBar
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.graphics.withClip
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource

private const val BAR_HEIGHT_DP = 4f

/** The range a screen reader is told the bar runs over. */
private const val PERCENT = 100f

/** The gap between the filled part and the rest of the track, and the stop dot's size: Material 3's own. */
private const val GAP_DP = 4f

/**
 * A transfer line's bar: how much of its batch's bytes are done, filled in the accent over a
 * track of the accent's tint, or moving while there is no share to show, as the line for
 * syncing has none. Screen readers name it [description], the line's words. A share reads from
 * [share]'s latest value each display frame while its bytes
 * move: arriving bytes recompose nothing.
 */
@Composable
internal fun ShareBar(
    share: LatestSource<Double>?,
    description: String,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val track = accent.copy(alpha = ThemeOpacity.tintStrong)
    if (share == null) {
        LinearProgressIndicator(
            modifier = modifier.fillMaxWidth().semantics { contentDescription = description },
            color = accent,
            trackColor = track,
        )
    } else {
        AndroidView(
            modifier = modifier.fillMaxWidth(),
            factory = { context -> ShareBarView(context) },
            update = { view ->
                view.contentDescription = description
                view.applyColors(accent = accent.toArgb(), track = track.toArgb())
                view.follow(share)
            },
            onRelease = { view -> view.stopFollowing() },
        )
    }
}

/** How long the moving bar's piece takes to cross the track. */
private const val MOVING_MILLIS = 1500L

/** The moving bar's piece, as a share of the track's width. */
private const val MOVING_SHARE = 0.4f

/**
 * Material 3's linear progress bar drawn from a fraction the View reads itself: four tall, rounded,
 * the filled part, a gap, the rest of the track and a dot at its end that marks where the bar stops.
 * [showMoving] swaps the fraction for a piece that crosses the track again and again, for a step that
 * counts nothing.
 */
internal class ShareBarView(
    context: Context,
) : View(context) {
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val track = Paint(Paint.ANTI_ALIAS_FLAG)
    private val following = FrameFollowing<Double>(this, ::show)
    private var fraction = 0f
    private val bounds = RectF()
    private var phase = 0f
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

    init {
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
    }

    /** A progress bar of a percent, so a screen reader says how much is done. */
    override fun onInitializeAccessibilityNodeInfo(info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        info.className = ProgressBar::class.java.name
        info.rangeInfo =
            AccessibilityNodeInfo.RangeInfo.obtain(
                AccessibilityNodeInfo.RangeInfo.RANGE_TYPE_PERCENT,
                0f,
                PERCENT,
                fraction * PERCENT,
            )
    }

    fun applyColors(
        accent: Int,
        track: Int,
    ) {
        if (fill.color == accent && this.track.color == track) return
        fill.color = accent
        this.track.color = track
        invalidate()
    }

    fun follow(share: LatestSource<Double>) = following.follow(share)

    fun stopFollowing() = following.stop()

    /** Shows [share] of the track filled, without following a stream. */
    fun showFraction(share: Float) {
        crossing.cancel()
        fraction = share.coerceIn(0f, 1f)
        invalidate()
    }

    /** Shows a piece crossing the track, while a step counts nothing. */
    fun showMoving() {
        if (!crossing.isStarted) crossing.start()
    }

    override fun onDetachedFromWindow() {
        crossing.cancel()
        super.onDetachedFromWindow()
    }

    private fun show(share: Double?) {
        fraction = (share ?: 0.0).coerceIn(0.0, 1.0).toFloat()
        invalidate()
    }

    override fun onMeasure(
        widthMeasureSpec: Int,
        heightMeasureSpec: Int,
    ) = setMeasuredDimension(MeasureSpec.getSize(widthMeasureSpec), dp(BAR_HEIGHT_DP).toInt())

    override fun onDraw(canvas: Canvas) {
        val height = dp(BAR_HEIGHT_DP)
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

    private fun dp(value: Float): Float {
        val metrics = resources.displayMetrics
        return TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value, metrics)
    }
}
