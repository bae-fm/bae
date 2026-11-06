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
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource

private const val BAR_MAX = 10_000

/**
 * What shows while a long call runs: [idle] until the call runs a step, then
 * the step's [title] over a bar of its [fraction] done, or a moving bar for a
 * step that counts nothing (null), in its place, which a View reads from
 * [steps]'s latest value each display frame. Only a step starting or ending
 * recomposes, and tells [onStepping]. With no call to follow, [idle] alone.
 */
@Composable
internal fun <Step : Any> StepProgressLine(
    steps: LatestSource<Step>?,
    title: (Step) -> String,
    fraction: (Step) -> Double?,
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
                factory = { context -> StepProgressView<Step>(context) },
                update = { view ->
                    view.applyStyle(accent = accent, track = track, text = text, textSizeSp = size)
                    view.describe(title, fraction)
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

/** A call's step over its bar, gone between steps. */
internal class StepProgressView<Step : Any>(
    context: Context,
) : LinearLayout(context) {
    private val label = TextView(context)
    private val bar =
        ProgressBar(context, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = BAR_MAX
            isIndeterminate = false
        }
    private val following = FrameFollowing<Step>(this, ::show)
    private lateinit var title: (Step) -> String
    private lateinit var fraction: (Step) -> Double?

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

    /** How a step reads: its name, and how much of it is done, null when it counts nothing; told before [follow]. */
    fun describe(
        title: (Step) -> String,
        fraction: (Step) -> Double?,
    ) {
        this.title = title
        this.fraction = fraction
    }

    /** Follow [steps]; a new call starts from nothing told, so its first show tells [onStepping]. */
    fun follow(steps: LatestSource<Step>) {
        if (!following.follows(steps)) told = null
        following.follow(steps)
    }

    fun stopFollowing() = following.stop()

    private fun show(step: Step?) {
        if (step != null) {
            val shown = title(step)
            if (label.text.toString() != shown) label.text = shown
            val done = fraction(step)
            val counted = done != null
            if (bar.isIndeterminate == counted) bar.isIndeterminate = !counted
            if (done != null) bar.progress = (done * BAR_MAX).toInt()
        }
        val started = step != null
        visibility = if (started) View.VISIBLE else View.GONE
        if (started == told) return
        told = started
        // FrameFollowing can show during a View update, which mustn't change
        // composition state itself.
        post { onStepping(started) }
    }
}
