package fm.bae.app.ui.playback

import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.util.LayoutDirection
import uniffi.bae_bridge.BridgeTrackSpan

/**
 * The parts of a track whose bytes this device holds, each a rounded stretch
 * of the seek bar's line, as fractions of its length from the start the
 * layout reads from.
 */
internal class HeldRangesDrawable : Drawable() {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val stretch = RectF()

    /** The stretches drawn, as core sent them. */
    internal var ranges: List<BridgeTrackSpan> = emptyList()
        private set

    var color: Int
        get() = paint.color
        set(value) {
            if (paint.color == value) return
            paint.color = value
            invalidateSelf()
        }

    /** Draw [ranges] from now on. */
    fun show(ranges: List<BridgeTrackSpan>) {
        if (ranges == this.ranges) return
        this.ranges = ranges
        invalidateSelf()
    }

    override fun draw(canvas: Canvas) {
        val line = bounds
        val width = line.width().toFloat()
        val corner = line.height() / 2f
        val rightToLeft = layoutDirection == LayoutDirection.RTL
        for (range in ranges) {
            val from = (range.start * width).toFloat()
            val to = (range.end * width).toFloat()
            if (rightToLeft) {
                stretch.set(line.right - to, line.top.toFloat(), line.right - from, line.bottom.toFloat())
            } else {
                stretch.set(line.left + from, line.top.toFloat(), line.left + to, line.bottom.toFloat())
            }
            canvas.drawRoundRect(stretch, corner, corner, paint)
        }
    }

    override fun setAlpha(alpha: Int) {
        paint.alpha = alpha
        invalidateSelf()
    }

    override fun setColorFilter(colorFilter: ColorFilter?) {
        paint.colorFilter = colorFilter
        invalidateSelf()
    }

    @Deprecated("Deprecated in Java")
    override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
}
