package fm.bae.app.ui.sync

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.util.TypedValue
import android.view.View
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource

private const val BAR_HEIGHT_DP = 4f

/** The gap between the filled part and the rest of the track, and the stop dot's size: Material 3's own. */
private const val GAP_DP = 4f

/**
 * A transfer line's bar: how much of its batch's bytes are done, filled in the accent over a
 * track of the accent's tint, or moving while there is no share to show, as the line for
 * syncing has none. A share reads from [share]'s latest value each display frame while its bytes
 * move: arriving bytes recompose nothing.
 */
@Composable
internal fun ShareBar(
    share: LatestSource<Double>?,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val track = accent.copy(alpha = ThemeOpacity.tintStrong)
    if (share == null) {
        LinearProgressIndicator(modifier = modifier.fillMaxWidth(), color = accent, trackColor = track)
    } else {
        AndroidView(
            modifier = modifier.fillMaxWidth(),
            factory = { context -> ShareBarView(context) },
            update = { view ->
                view.applyColors(accent = accent.toArgb(), track = track.toArgb())
                view.follow(share)
            },
            onRelease = { view -> view.stopFollowing() },
        )
    }
}

/**
 * Material 3's linear progress bar drawn from a fraction the View reads itself: four tall, rounded,
 * the filled part, a gap, the rest of the track and a dot at its end that marks where the bar stops.
 */
internal class ShareBarView(
    context: Context,
) : View(context) {
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val track = Paint(Paint.ANTI_ALIAS_FLAG)
    private val following = FrameFollowing<Double>(this, ::show)
    private var fraction = 0f
    private val bounds = RectF()

    init {
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
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
