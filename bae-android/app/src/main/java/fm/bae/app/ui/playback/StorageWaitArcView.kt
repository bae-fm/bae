package fm.bae.app.ui.playback

import android.animation.ObjectAnimator
import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.util.TypedValue
import android.view.View
import android.view.animation.LinearInterpolator

private const val ARC_STROKE_DP = 2.5f
private const val ARC_SWEEP_DEGREES = 90f

/** Where the arc starts before it turns: the top. */
private const val ARC_START_DEGREES = -90f
private const val TURN_MS = 1_000L

/**
 * An arc turning around the play/pause control, shown while the track's
 * load waits on storage: it turns while it is attached to a window.
 */
internal class StorageWaitArcView(
    context: Context,
) : View(context) {
    private val paint =
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeWidth =
                TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, ARC_STROKE_DP, resources.displayMetrics)
        }
    private val turning =
        ObjectAnimator.ofFloat(this, ROTATION, 0f, -360f).apply {
            duration = TURN_MS
            repeatCount = ValueAnimator.INFINITE
            interpolator = LinearInterpolator()
        }

    init {
        accessibilityLiveRegion = ACCESSIBILITY_LIVE_REGION_POLITE
    }

    /** Whether the arc turns now: from when the view is attached to a window until it is detached. */
    internal val isTurning: Boolean
        get() = turning.isStarted

    /** Draw in [color], read out as [label]. */
    fun style(
        color: Int,
        label: String,
    ) {
        contentDescription = label
        if (paint.color == color) return
        paint.color = color
        invalidate()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        turning.start()
    }

    override fun onDetachedFromWindow() {
        turning.cancel()
        rotation = 0f
        super.onDetachedFromWindow()
    }

    override fun onDraw(canvas: Canvas) {
        val inset = paint.strokeWidth / 2f
        canvas.drawArc(inset, inset, width - inset, height - inset, ARC_START_DEGREES, ARC_SWEEP_DEGREES, false, paint)
    }
}
