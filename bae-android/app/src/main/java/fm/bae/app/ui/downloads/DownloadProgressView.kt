package fm.bae.app.ui.downloads

import android.content.Context
import android.util.TypedValue
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.factsLine
import fm.bae.app.formatFileSize
import fm.bae.app.requireDisplayableByteCount
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidProgressBarView
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeDownloadTransferProgress

/**
 * A download's progress bar over its "{done} of {total}" byte line, led by
 * [label] ("Pinning · 120 MB of 300 MB") when there is one, which a View reads
 * from [progress]'s latest value each display frame while the release
 * downloads: arriving bytes recompose nothing. Screen readers name the bar
 * Pinning, and read it as a progress bar of a percent. [gap] spaces the bar from the line
 * under it. The bar ends in an accent dot.
 */
@Composable
internal fun DownloadProgress(
    progress: LatestSource<BridgeDownloadTransferProgress>,
    label: String?,
    gap: Dp,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val text = MaterialTheme.colorScheme.onSurfaceVariant.toArgb()
    val size = ThemeText.fine.style.fontSize.value
    val name = stringResource(R.string.pinning)
    AndroidView(
        modifier = modifier,
        factory = { context -> DownloadProgressView(context) },
        update = { view ->
            view.applyStyle(accent = accent, text = text, textSizeSp = size)
            view.name = name
            view.gap = gap
            view.label = label
            view.follow(progress)
        },
        onRelease = { view -> view.stopFollowing() },
    )
}

internal class DownloadProgressView(
    context: Context,
) : LinearLayout(context) {
    private val bar = AndroidProgressBarView(context)
    private val bytes = TextView(context).apply { includeFontPadding = false }
    private val following = FrameFollowing<BridgeDownloadTransferProgress>(this, ::show)

    /** The words that lead the byte line, or null for the bytes alone. */
    var label: String? = null

    /** What screen readers call the bar. */
    var name: String = ""
        set(value) {
            field = value
            bar.contentDescription = value
        }

    /** The space between the bar and the line under it. */
    var gap: Dp = Dp.Unspecified
        set(value) {
            field = value
            (bar.layoutParams as LayoutParams).bottomMargin = (value.value * resources.displayMetrics.density).toInt()
            bar.requestLayout()
        }

    init {
        orientation = VERTICAL
        layoutParams = LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        addView(bar, LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        addView(bytes)
    }

    fun applyStyle(
        accent: Color,
        text: Int,
        textSizeSp: Float,
    ) {
        bar.applyAccent(accent)
        bytes.setTextColor(text)
        bytes.setTextSize(TypedValue.COMPLEX_UNIT_SP, textSizeSp)
    }

    fun follow(progress: LatestSource<BridgeDownloadTransferProgress>) = following.follow(progress)

    fun stopFollowing() = following.stop()

    private fun show(progress: BridgeDownloadTransferProgress?) {
        bar.showFraction((progress?.fraction ?: 0.0).toFloat())
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
