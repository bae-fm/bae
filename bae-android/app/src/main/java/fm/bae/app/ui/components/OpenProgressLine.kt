package fm.bae.app.ui.components

import android.content.Context
import android.content.res.ColorStateList
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.coreString
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeOpenStep
import uniffi.bae_bridge.bridgeOpenStepKey

private const val BAR_MAX = 10_000

/**
 * What shows while a library opens: [idle] until the open runs a step, then
 * the step's name over a bar of how much of it is done, in its place, which a
 * View reads from [steps]'s latest value each display frame. Only a step
 * starting or ending recomposes, and tells [onStepping]. With no open to
 * follow, [idle] alone.
 */
@Composable
internal fun OpenProgressLine(
    steps: LatestSource<BridgeOpenProgress>?,
    modifier: Modifier = Modifier,
    onStepping: (Boolean) -> Unit = {},
    idle: @Composable () -> Unit,
) {
    var stepping by remember(steps) { mutableStateOf(false) }
    Column(modifier = modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        if (!stepping || steps == null) idle()
        if (steps != null) {
            val accent = MaterialTheme.colorScheme.primary.toArgb()
            val track = BaeTheme.colors.hairline.toArgb()
            val text = MaterialTheme.colorScheme.onSurfaceVariant.toArgb()
            val size = ThemeText.body.style.fontSize.value
            AndroidView(
                modifier = Modifier.fillMaxWidth(),
                factory = { context -> OpenProgressView(context) },
                update = { view ->
                    view.applyStyle(accent = accent, track = track, text = text, textSizeSp = size)
                    view.onStepping = { started ->
                        stepping = started
                        onStepping(started)
                    }
                    view.follow(steps)
                },
                onRelease = { view -> view.stopFollowing() },
            )
        }
    }
}

/** The open's step over its bar, gone between steps. */
internal class OpenProgressView(
    context: Context,
) : LinearLayout(context) {
    private val label = TextView(context)
    private val bar =
        ProgressBar(context, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = BAR_MAX
            isIndeterminate = false
        }
    private val following = FrameFollowing<BridgeOpenProgress>(this, ::show)

    /** Whether a step runs, as last told to [onStepping]; null until the View shows what its stream holds. */
    private var told: Boolean? = null
    var onStepping: (Boolean) -> Unit = {}

    init {
        orientation = VERTICAL
        layoutParams = LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        addView(label)
        addView(bar, LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        visibility = View.GONE
    }

    fun applyStyle(
        accent: Int,
        track: Int,
        text: Int,
        textSizeSp: Float,
    ) {
        bar.progressTintList = ColorStateList.valueOf(accent)
        bar.progressBackgroundTintList = ColorStateList.valueOf(track)
        label.setTextColor(text)
        label.setTextSize(TypedValue.COMPLEX_UNIT_SP, textSizeSp)
    }

    /** Follow [steps]; a new open starts from nothing told, so its first show tells [onStepping]. */
    fun follow(steps: LatestSource<BridgeOpenProgress>) {
        if (!following.follows(steps)) told = null
        following.follow(steps)
    }

    fun stopFollowing() = following.stop()

    private fun show(progress: BridgeOpenProgress?) {
        if (progress != null) {
            val title = context.coreString(bridgeOpenStepKey(progress.step))
            if (label.text.toString() != title) label.text = title
            val fraction = if (progress.total > 0uL) progress.done.toDouble() / progress.total.toDouble() else 0.0
            bar.progress = (fraction * BAR_MAX).toInt()
        }
        val started = progress != null
        visibility = if (started) View.VISIBLE else View.GONE
        if (started == told) return
        told = started
        // FrameFollowing can show during a View update, which mustn't change
        // composition state itself.
        post { onStepping(started) }
    }
}

@Preview(showBackground = true)
@Composable
private fun OpenProgressLineMigratingPreview() {
    BaeTheme {
        OpenProgressLine(
            steps = LatestStream.openProgress(BridgeOpenProgress(BridgeOpenStep.MIGRATING, 1uL, 3uL)).only,
            modifier = Modifier.widthIn(max = 360.dp).padding(16.dp),
        ) { CircularProgressIndicator() }
    }
}

@Preview(showBackground = true)
@Composable
private fun OpenProgressLineRebuildingPreview() {
    BaeTheme {
        OpenProgressLine(
            steps = LatestStream.openProgress(BridgeOpenProgress(BridgeOpenStep.REBUILDING, 4uL, 9uL)).only,
            modifier = Modifier.widthIn(max = 360.dp).padding(16.dp),
        ) { CircularProgressIndicator() }
    }
}

@Preview(showBackground = true)
@Composable
private fun OpenProgressLineBetweenStepsPreview() {
    BaeTheme {
        OpenProgressLine(
            steps = LatestStream.openProgress().only,
            modifier = Modifier.widthIn(max = 360.dp).padding(16.dp),
        ) { CircularProgressIndicator() }
    }
}
