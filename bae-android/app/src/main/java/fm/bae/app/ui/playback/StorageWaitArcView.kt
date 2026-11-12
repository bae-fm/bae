package fm.bae.app.ui.playback

import android.animation.ObjectAnimator
import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.util.TypedValue
import android.view.View
import android.view.animation.LinearInterpolator
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource

private const val ARC_STROKE_DP = 2.5f
private const val ARC_SWEEP_DEGREES = 90f

/** Where the arc starts before it turns: the top. */
private const val ARC_START_DEGREES = -90f
private const val TURN_MS = 1_000L

/**
 * An arc turning around the play/pause control while the track's load
 * waits on storage, gone otherwise. It reads the wait each display frame
 * from the stream's key, so the wait coming and going never recomposes.
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
    private val following = FrameFollowing<Boolean>(this, ::show)

    init {
        visibility = GONE
        accessibilityLiveRegion = ACCESSIBILITY_LIVE_REGION_POLITE
    }

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

    fun follow(waits: LatestSource<Boolean>) = following.follow(waits)

    fun stopFollowing() {
        following.stop()
        show(null)
    }

    private fun show(waits: Boolean?) {
        val shown = waits == true
        visibility = if (shown) VISIBLE else GONE
        if (shown && !turning.isStarted) {
            turning.start()
        } else if (!shown) {
            turning.cancel()
            rotation = 0f
        }
    }

    override fun onDraw(canvas: Canvas) {
        val inset = paint.strokeWidth / 2f
        canvas.drawArc(inset, inset, width - inset, height - inset, ARC_START_DEGREES, ARC_SWEEP_DEGREES, false, paint)
    }
}
