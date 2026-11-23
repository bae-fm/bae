package fm.bae.app.ui.components

import android.animation.ValueAnimator
import android.content.Context
import android.database.ContentObserver
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.util.TypedValue
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.view.animation.LinearInterpolator
import android.widget.ProgressBar
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.core.graphics.withClip
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource

/** How tall a bar's track is. */
private const val BAR_HEIGHT_DP = 4f

/** The range that the bar reports to a screen reader. */
private const val PERCENT = 100f

/** The gap between the filled part and the rest of the track, and the stop dot's size: Material 3's own. */
private const val GAP_DP = 4f

/** How long the moving bar's piece takes to cross the track. */
private const val MOVING_MILLIS = 1500L

/** The moving bar's piece, as a share of the track's width. */
private const val MOVING_SHARE = 0.3f

/** Where the piece's start rests, as a share of the track's width, before the bar first moves and in a still frame. */
private const val MOVING_REST_START = 0.38f

/** [MOVING_REST_START] as the phase that puts the piece's start there: the piece spans [MOVING_SHARE] of the track. */
private const val MOVING_REST_PHASE = (MOVING_REST_START + MOVING_SHARE) / (1f + MOVING_SHARE)

/**
 * Material 3's linear progress bar: 4 dp tall with rounded ends, the filled part, a gap, the rest of
 * the track and a dot at its end that marks where the bar stops. The bar shows the fraction that
 * callers pass to [showFraction], or follows a stream's value after [follow]. [showMoving] swaps the
 * fraction for a piece that crosses the track again and again, for a step that counts nothing.
 */
internal class AndroidProgressBarView(
    context: Context,
) : View(context) {
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val track = Paint(Paint.ANTI_ALIAS_FLAG)
    private val bounds = RectF()
    private val following =
        FrameFollowing<Double>(this) { share ->
            if (share == null) clearProgress() else showFraction(share.toFloat())
        }
    private var progress: Progress? = null
        set(value) {
            field = value
            importantForAccessibility =
                if (value == null) IMPORTANT_FOR_ACCESSIBILITY_NO else IMPORTANT_FOR_ACCESSIBILITY_YES
            invalidate()
        }

    private sealed interface Progress {
        data class Counted(
            val fraction: Float,
        ) : Progress

        class Moving : Progress {
            var phase = MOVING_REST_PHASE
            var stopObservingScale: (() -> Unit)? = null
        }
    }

    private val crossing =
        ValueAnimator.ofFloat(MOVING_REST_PHASE, MOVING_REST_PHASE + 1f).apply {
            duration = MOVING_MILLIS
            repeatCount = ValueAnimator.INFINITE
            interpolator = LinearInterpolator()
            addUpdateListener {
                (progress as? Progress.Moving)?.let { moving ->
                    moving.phase = (it.animatedValue as Float) % 1f
                    invalidate()
                }
            }
        }

    init {
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
    }

    /** Reports the bar to a screen reader as a progress bar of a percent. */
    override fun onInitializeAccessibilityNodeInfo(info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        info.className = ProgressBar::class.java.name
        (progress as? Progress.Counted)?.let { counted ->
            info.rangeInfo =
                AccessibilityNodeInfo.RangeInfo.obtain(
                    AccessibilityNodeInfo.RangeInfo.RANGE_TYPE_PERCENT,
                    0f,
                    PERCENT,
                    counted.fraction * PERCENT,
                )
        }
    }

    fun applyAccent(accent: Color) {
        val fillColor = accent.toArgb()
        val trackColor = accent.copy(alpha = ThemeOpacity.tintStrong).toArgb()
        if (fill.color == fillColor && track.color == trackColor) return
        fill.color = fillColor
        track.color = trackColor
        invalidate()
    }

    /** Shows the latest value [share] holds, read each display frame while core sends new ones. */
    fun follow(share: LatestSource<Double>) = following.follow(share)

    fun stopFollowing() = following.stop()

    /** Shows [share] of the track filled. */
    fun showFraction(share: Float) {
        if ((progress as? Progress.Counted)?.fraction == share) return
        stopCrossing()
        progress = Progress.Counted(share)
    }

    /** Hides the bar while the byte measurement it needs is absent. */
    fun clearProgress() {
        stopCrossing()
        progress = null
    }

    /** Shows a piece crossing the track, while a step counts nothing. */
    fun showMoving() {
        if (progress is Progress.Moving) return
        val moving = Progress.Moving()
        progress = moving
        if (isAttachedToWindow) observeCrossing(moving)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        (progress as? Progress.Moving)?.let(::observeCrossing)
    }

    override fun onDetachedFromWindow() {
        stopCrossing()
        super.onDetachedFromWindow()
    }

    private fun observeCrossing(moving: Progress.Moving) {
        moving.stopObservingScale = observeAnimatorScale { updateCrossing(moving) }
        updateCrossing(moving)
    }

    private fun stopCrossing() {
        val moving = progress as? Progress.Moving ?: return
        moving.stopObservingScale?.invoke()
        moving.stopObservingScale = null
        crossing.cancel()
    }

    private fun updateCrossing(moving: Progress.Moving) {
        if (progress !== moving || !isAttachedToWindow) return
        if (ValueAnimator.areAnimatorsEnabled()) {
            if (!crossing.isStarted) crossing.start()
        } else {
            crossing.cancel()
            moving.phase = MOVING_REST_PHASE
            invalidate()
        }
    }

    private fun observeAnimatorScale(onScaleChange: () -> Unit): () -> Unit {
        val main = Handler(Looper.getMainLooper())
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            val listener = ValueAnimator.DurationScaleChangeListener { main.post { onScaleChange() } }
            ValueAnimator.registerDurationScaleChangeListener(listener)
            return { ValueAnimator.unregisterDurationScaleChangeListener(listener) }
        }
        val resolver = context.contentResolver
        val observer =
            object : ContentObserver(main) {
                override fun onChange(selfChange: Boolean) = onScaleChange()
            }
        val scaleUri = Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE)
        resolver.registerContentObserver(scaleUri, false, observer)
        return { resolver.unregisterContentObserver(observer) }
    }

    override fun onMeasure(
        widthMeasureSpec: Int,
        heightMeasureSpec: Int,
    ) = setMeasuredDimension(MeasureSpec.getSize(widthMeasureSpec), barHeight().toInt())

    /** The track's height, in pixels. */
    private fun barHeight(): Float = dp(BAR_HEIGHT_DP)

    private fun dp(value: Float): Float {
        val metrics = resources.displayMetrics
        return TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value, metrics)
    }

    override fun onDraw(canvas: Canvas) {
        val shown = progress ?: return
        val height = barHeight()
        val gap = dp(GAP_DP)
        val radius = height / 2f
        if (shown is Progress.Moving) {
            val piece = width * MOVING_SHARE
            val start = -piece + (width + piece) * shown.phase
            val leftEnd = minOf(start - gap, width.toFloat())
            if (leftEnd > 0f) {
                canvas.drawRoundRect(bounds.apply { set(0f, 0f, leftEnd, height) }, radius, radius, track)
            }
            val rightStart = maxOf(start + piece + gap, 0f)
            if (rightStart < width) {
                val rest = bounds.apply { set(rightStart, 0f, width.toFloat(), height) }
                canvas.drawRoundRect(rest, radius, radius, track)
            }
            canvas.withClip(0f, 0f, width.toFloat(), height) {
                drawRoundRect(bounds.apply { set(start, 0f, start + piece, height) }, radius, radius, fill)
            }
            canvas.drawCircle(width - radius, radius, radius, fill)
            return
        }
        val fraction = (shown as Progress.Counted).fraction
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
