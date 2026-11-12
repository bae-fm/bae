package fm.bae.app.ui.downloads

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.coreString
import fm.bae.app.factsLine
import fm.bae.app.formatFileSize
import fm.bae.app.requireDisplayableByteCount
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeDownloadTransferProgress

private const val BAR_HEIGHT_DP = 4f
private const val BAR_GAP_DP = 4f
private const val BAR_MARGIN_TOP_DP = 4
private const val BAR_MARGIN_BOTTOM_DP = 2

/**
 * A download's progress bar over its "{done} of {total}" byte line, led by
 * [label] ("Pinning · 120 MB of 300 MB") when there is one, which a View reads
 * from [progress]'s latest value each display frame while the release
 * downloads: arriving bytes recompose nothing. With [stopDot] the bar ends in a
 * dot, as the album page's bar does.
 */
@Composable
internal fun DownloadProgress(
    progress: LatestSource<BridgeDownloadTransferProgress>,
    label: String?,
    modifier: Modifier = Modifier,
    stopDot: Boolean = false,
) {
    val accent = MaterialTheme.colorScheme.primary.toArgb()
    val text = MaterialTheme.colorScheme.onSurfaceVariant.toArgb()
    val size = ThemeText.fine.style.fontSize.value
    AndroidView(
        modifier = modifier,
        factory = { context -> DownloadProgressView(context) },
        update = { view ->
            view.applyStyle(accent = accent, text = text, textSizeSp = size, stopDot = stopDot)
            view.label = label
            view.follow(progress)
        },
        onRelease = { view -> view.stopFollowing() },
    )
}

/**
 * A bar 4dp high with round ends: the fill in the accent, a 4dp gap, then the
 * rest of the track in the accent's strong tint, and with [stopDot] a 4dp dot at
 * its far end.
 */
internal class PinBarView(
    context: Context,
) : View(context) {
    private val density = resources.displayMetrics.density
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val rect = RectF()
    private var accent = 0
    private var stopDot = false

    /** How much of the bar is filled, 0 to 1. */
    var fraction: Float = 0f
        set(value) {
            if (field == value) return
            field = value
            invalidate()
        }

    fun applyStyle(
        accent: Int,
        stopDot: Boolean,
    ) {
        if (this.accent == accent && this.stopDot == stopDot) return
        this.accent = accent
        this.stopDot = stopDot
        invalidate()
    }

    override fun onMeasure(
        widthMeasureSpec: Int,
        heightMeasureSpec: Int,
    ) = setMeasuredDimension(
        getDefaultSize(suggestedMinimumWidth, widthMeasureSpec),
        (BAR_HEIGHT_DP * density).toInt(),
    )

    override fun onDraw(canvas: Canvas) {
        val height = BAR_HEIGHT_DP * density
        val gap = BAR_GAP_DP * density
        val radius = height / 2
        val width = this.width.toFloat()
        val fillEnd = (width * fraction).coerceIn(0f, width)
        val trackStart = if (fillEnd > 0f) minOf(fillEnd + gap, width) else 0f
        paint.color = Color(accent).copy(alpha = ThemeOpacity.tintStrong).toArgb()
        if (trackStart < width) {
            rect.set(trackStart, 0f, width, height)
            canvas.drawRoundRect(rect, radius, radius, paint)
        }
        paint.color = accent
        if (fillEnd > 0f) {
            rect.set(0f, 0f, fillEnd, height)
            canvas.drawRoundRect(rect, radius, radius, paint)
        }
        if (stopDot) {
            rect.set(width - height, 0f, width, height)
            canvas.drawRoundRect(rect, radius, radius, paint)
        }
    }
}

internal class DownloadProgressView(
    context: Context,
) : LinearLayout(context) {
    private val bar = PinBarView(context)
    private val bytes = TextView(context)
    private val following = FrameFollowing<BridgeDownloadTransferProgress>(this, ::show)

    /** The words that lead the byte line, or null for the bytes alone. */
    var label: String? = null

    init {
        orientation = VERTICAL
        layoutParams = LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        addView(
            bar,
            LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                topMargin = (BAR_MARGIN_TOP_DP * resources.displayMetrics.density).toInt()
                bottomMargin = (BAR_MARGIN_BOTTOM_DP * resources.displayMetrics.density).toInt()
            },
        )
        addView(bytes)
    }

    fun applyStyle(
        accent: Int,
        text: Int,
        textSizeSp: Float,
        stopDot: Boolean,
    ) {
        bar.applyStyle(accent, stopDot)
        bytes.setTextColor(text)
        bytes.setTextSize(TypedValue.COMPLEX_UNIT_SP, textSizeSp)
    }

    fun follow(progress: LatestSource<BridgeDownloadTransferProgress>) = following.follow(progress)

    fun stopFollowing() = following.stop()

    private fun show(progress: BridgeDownloadTransferProgress?) {
        bar.fraction = (progress?.fraction ?: 0.0).toFloat()
        val text =
            progress
                ?.let {
                    context.coreString(
                        "core.download.bytes_progress",
                        mapOf(
                            "done" to context.formatFileSize(it.bytesDone.requireDisplayableByteCount()),
                            "total" to context.formatFileSize(it.bytesTotal.requireDisplayableByteCount()),
                        ),
                    )
                }.orEmpty()
                .let { counts -> context.factsLine(listOf(label, counts)) }
        if (bytes.text.toString() != text) bytes.text = text
    }
}
