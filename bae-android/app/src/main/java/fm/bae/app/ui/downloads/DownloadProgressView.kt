package fm.bae.app.ui.downloads

import android.content.Context
import android.content.res.ColorStateList
import android.util.TypedValue
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.coreString
import fm.bae.app.formatFileSize
import fm.bae.app.requireDisplayableByteCount
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeDownloadTransferProgress

private const val BAR_MAX = 10_000

/**
 * A download's progress bar over its "{done} of {total}" byte line, which a
 * View reads from [progress]'s latest value each display frame while the
 * release downloads: arriving bytes recompose nothing.
 */
@Composable
internal fun DownloadProgress(
    progress: LatestSource<BridgeDownloadTransferProgress>,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary.toArgb()
    val track = BaeTheme.colors.hairline.toArgb()
    val text = MaterialTheme.colorScheme.onSurfaceVariant.toArgb()
    val size = ThemeText.fine.style.fontSize.value
    AndroidView(
        modifier = modifier,
        factory = { context -> DownloadProgressView(context) },
        update = { view ->
            view.applyStyle(accent = accent, track = track, text = text, textSizeSp = size)
            view.follow(progress)
        },
        onRelease = { view -> view.stopFollowing() },
    )
}

internal class DownloadProgressView(
    context: Context,
) : LinearLayout(context) {
    private val bar =
        ProgressBar(context, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = BAR_MAX
            isIndeterminate = false
        }
    private val bytes = TextView(context)
    private val following = FrameFollowing<BridgeDownloadTransferProgress>(this, ::show)

    init {
        orientation = VERTICAL
        layoutParams = LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        addView(bar, LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        addView(bytes)
    }

    fun applyStyle(
        accent: Int,
        track: Int,
        text: Int,
        textSizeSp: Float,
    ) {
        bar.progressTintList = ColorStateList.valueOf(accent)
        bar.progressBackgroundTintList = ColorStateList.valueOf(track)
        bytes.setTextColor(text)
        bytes.setTextSize(TypedValue.COMPLEX_UNIT_SP, textSizeSp)
    }

    fun follow(progress: LatestSource<BridgeDownloadTransferProgress>) = following.follow(progress)

    fun stopFollowing() = following.stop()

    private fun show(progress: BridgeDownloadTransferProgress?) {
        bar.progress = ((progress?.fraction ?: 0.0) * BAR_MAX).toInt()
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
        if (bytes.text.toString() != text) bytes.text = text
    }
}
