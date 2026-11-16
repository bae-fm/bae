package fm.bae.app.ui.sync

import android.content.Context
import android.graphics.Paint
import android.graphics.RectF
import android.util.TypedValue
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.ProgressBar

/** How tall a bar's track is. */
private const val BAR_HEIGHT_DP = 4f

/** The range that a bar reports to a screen reader. */
private const val PERCENT = 100f

/**
 * What every bar view shares: a track four tall that takes the width it is given, the paints of its
 * fill and its track, and a screen reader's view of it as a progress bar of the percent [shareDone]
 * gives.
 */
internal abstract class TrackBarView(
    context: Context,
) : View(context) {
    protected val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    protected val track = Paint(Paint.ANTI_ALIAS_FLAG)
    protected val bounds = RectF()

    /** The share of the bar done, from 0 to 1, which a screen reader reads as a percent. */
    protected abstract val shareDone: Float

    init {
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
    }

    /** Reports the bar to a screen reader as a progress bar of a percent. */
    override fun onInitializeAccessibilityNodeInfo(info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        info.className = ProgressBar::class.java.name
        info.rangeInfo =
            AccessibilityNodeInfo.RangeInfo.obtain(
                AccessibilityNodeInfo.RangeInfo.RANGE_TYPE_PERCENT,
                0f,
                PERCENT,
                shareDone * PERCENT,
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

    override fun onMeasure(
        widthMeasureSpec: Int,
        heightMeasureSpec: Int,
    ) = setMeasuredDimension(MeasureSpec.getSize(widthMeasureSpec), barHeight().toInt())

    /** The track's height, in pixels. */
    protected fun barHeight(): Float = dp(BAR_HEIGHT_DP)

    protected fun dp(value: Float): Float {
        val metrics = resources.displayMetrics
        return TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value, metrics)
    }
}
