package fm.bae.app.ui.playback

import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PixelFormat
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.util.LayoutDirection

/** The two ways a line with a position is drawn. */
enum class SeekLineLook {
    /** The bar's: a 4 dp line under the held parts, with a round 14 dp thumb on it. */
    BAR,

    /** The full player's: a 16 dp line cut by a 4 x 44 dp upright thumb, with an end dot and no held parts. */
    LARGE,
    ;

    /** How tall the thumb is, which stands out past the line. */
    val thumbHeightDp: Float
        get() =
            when (this) {
                BAR -> BAR_THUMB_DP
                LARGE -> LARGE_THUMB_HEIGHT_DP
            }

    /** How tall the row is that the design draws, which the line sits centred in. */
    val rowHeightDp: Float
        get() =
            when (this) {
                BAR -> BAR_THUMB_DP
                LARGE -> LARGE_LINE_DP
            }

    /** How tall the line's view is: Android's 48 dp touch target, or the thumb when that is taller. */
    val touchHeightDp: Float
        get() = maxOf(MIN_TOUCH_DP, thumbHeightDp)

    /** How far the thumb reaches past either side of the position it marks, so a view pads its line by it. */
    val thumbHalfWidthDp: Float
        get() =
            when (this) {
                BAR -> BAR_THUMB_DP / 2f
                LARGE -> LARGE_THUMB_WIDTH_DP / 2f
            }

    companion object {
        const val BAR_THUMB_DP = 14f
        const val LARGE_THUMB_WIDTH_DP = 4f
        const val LARGE_THUMB_HEIGHT_DP = 44f
        const val LARGE_LINE_DP = 16f
        private const val MIN_TOUCH_DP = 48f
    }
}

private const val BAR_LINE_DP = 4f

/** The least the played part of a bar's line shows, so a start still shows where it is. */
private const val BAR_PLAYED_MIN_DP = 5f

/** The clear space around the large thumb, where neither the played part nor the rest of the line runs. */
private const val LARGE_GAP_DP = 6f
private const val LARGE_END_RADIUS_DP = 8f
private const val LARGE_GAP_RADIUS_DP = 2f
private const val LARGE_END_DOT_DP = 4f
private const val LARGE_END_DOT_INSET_DP = 6f
private const val MAX_LEVEL = 10_000f
private const val DISABLED_ALPHA = 97

/**
 * A seek line: the track, what has played of it up to the position its level gives (0 to 10,000),
 * and the thumb, in one [look]. It is a seek bar's whole drawing (the bar has no thumb of its own),
 * so it knows where the thumb stands and cuts the line around it.
 */
internal class SeekLineDrawable(
    private val density: Float,
    private val look: SeekLineLook,
) : Drawable() {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val shape = RectF()
    private val path = Path()
    private var fraction = 0f
    private var enabled = true

    /** The parts of the track whose bytes are here; a [SeekLineLook.BAR] shows them under the played part. */
    internal val heldRanges = HeldRangesDrawable()

    /** The played part, the thumb and the end dot. */
    var accent: Int = 0
        set(value) {
            if (field == value) return
            field = value
            invalidateSelf()
        }

    /** The line the played part runs along. */
    var track: Int = 0
        set(value) {
            if (field == value) return
            field = value
            invalidateSelf()
        }

    override fun isStateful(): Boolean = true

    override fun getIntrinsicHeight(): Int = dp(look.touchHeightDp).toInt()

    override fun onLevelChange(level: Int): Boolean {
        fraction = level / MAX_LEVEL
        invalidateSelf()
        return true
    }

    override fun onStateChange(stateSet: IntArray): Boolean {
        val nowEnabled = android.R.attr.state_enabled in stateSet
        if (nowEnabled == enabled) return false
        enabled = nowEnabled
        invalidateSelf()
        return true
    }

    override fun onLayoutDirectionChanged(layoutDirection: Int): Boolean {
        heldRanges.setLayoutDirection(layoutDirection)
        return true
    }

    override fun draw(canvas: Canvas) {
        val rightToLeft = layoutDirection == LayoutDirection.RTL
        val dimmed = !enabled
        if (dimmed) canvas.saveLayerAlpha(null, DISABLED_ALPHA)
        when (look) {
            SeekLineLook.BAR -> drawBar(canvas, rightToLeft)
            SeekLineLook.LARGE -> drawLarge(canvas, rightToLeft)
        }
        if (dimmed) canvas.restore()
    }

    private fun drawBar(
        canvas: Canvas,
        rightToLeft: Boolean,
    ) {
        val area = bounds
        val half = dp(BAR_LINE_DP) / 2f
        val width = area.width().toFloat()
        val centreY = area.exactCenterY()
        shape.set(area.left.toFloat(), centreY - half, area.right.toFloat(), centreY + half)
        paint.color = track
        canvas.drawRoundRect(shape, half, half, paint)
        heldRanges.setBounds(area.left, shape.top.toInt(), area.right, shape.bottom.toInt())
        heldRanges.draw(canvas)
        withMirroring(canvas, rightToLeft) {
            val playedWidth = maxOf(fraction * width, dp(BAR_PLAYED_MIN_DP))
            shape.set(area.left.toFloat(), centreY - half, area.left + playedWidth, centreY + half)
            paint.color = accent
            canvas.drawRoundRect(shape, half, half, paint)
            canvas.drawCircle(area.left + fraction * width, centreY, dp(SeekLineLook.BAR_THUMB_DP) / 2f, paint)
        }
    }

    private fun drawLarge(
        canvas: Canvas,
        rightToLeft: Boolean,
    ) {
        val area = bounds
        val left = area.left.toFloat()
        val right = area.right.toFloat()
        val centreY = area.exactCenterY()
        val lineHalf = dp(SeekLineLook.LARGE_LINE_DP) / 2f
        val thumbHalf = dp(SeekLineLook.LARGE_THUMB_WIDTH_DP) / 2f
        val thumbX = left + fraction * area.width()
        val playedEnd = thumbX - thumbHalf - dp(LARGE_GAP_DP)
        val restStart = thumbX + thumbHalf + dp(LARGE_GAP_DP)
        val end = dp(LARGE_END_RADIUS_DP)
        val gap = dp(LARGE_GAP_RADIUS_DP)
        withMirroring(canvas, rightToLeft) {
            if (playedEnd > left) {
                shape.set(left, centreY - lineHalf, playedEnd, centreY + lineHalf)
                paint.color = accent
                canvas.drawPath(roundedSides(shape, startCorner = end, endCorner = gap), paint)
            }
            if (restStart < right) {
                shape.set(restStart, centreY - lineHalf, right, centreY + lineHalf)
                paint.color = track
                canvas.drawPath(roundedSides(shape, startCorner = gap, endCorner = end), paint)
            }
            val dot = dp(LARGE_END_DOT_DP) / 2f
            val dotX = right - dp(LARGE_END_DOT_INSET_DP) - dot
            if (restStart < dotX - dot) {
                paint.color = accent
                canvas.drawCircle(dotX, centreY, dot, paint)
            }
            val thumbReach = dp(SeekLineLook.LARGE_THUMB_HEIGHT_DP) / 2f
            shape.set(thumbX - thumbHalf, centreY - thumbReach, thumbX + thumbHalf, centreY + thumbReach)
            paint.color = accent
            canvas.drawRoundRect(shape, thumbHalf, thumbHalf, paint)
        }
    }

    /** [rect] with [startCorner] on its left corners and [endCorner] on its right ones. */
    private fun roundedSides(
        rect: RectF,
        startCorner: Float,
        endCorner: Float,
    ): Path =
        path.apply {
            rewind()
            addRoundRect(
                rect,
                floatArrayOf(
                    startCorner,
                    startCorner,
                    endCorner,
                    endCorner,
                    endCorner,
                    endCorner,
                    startCorner,
                    startCorner,
                ),
                Path.Direction.CW,
            )
        }

    /** Draw [block] mirrored across the line's middle when the layout reads right to left. */
    private inline fun withMirroring(
        canvas: Canvas,
        rightToLeft: Boolean,
        block: () -> Unit,
    ) {
        if (!rightToLeft) {
            block()
            return
        }
        canvas.save()
        canvas.scale(-1f, 1f, bounds.exactCenterX(), bounds.exactCenterY())
        block()
        canvas.restore()
    }

    private fun dp(value: Float): Float = value * density

    // The colours are set outright, so neither an alpha nor a filter applies.
    override fun setAlpha(alpha: Int) = Unit

    override fun setColorFilter(colorFilter: ColorFilter?) = Unit

    @Deprecated("Deprecated in Java")
    override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
}
