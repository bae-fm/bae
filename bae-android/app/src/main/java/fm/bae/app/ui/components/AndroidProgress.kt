package fm.bae.app.ui.components

import android.content.Context
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.sync.MovingBar
import fm.bae.app.ui.sync.ShareBarView
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeOpenProgress

/**
 * What shows while a long call runs: [idle] until the call runs a step, then
 * the step's [title] over a bar of its [fraction] done, or a moving bar for a
 * step that counts nothing (null), in its place, which a View reads from
 * [steps]'s latest value each display frame. Only a step starting or ending
 * recomposes, and tells [onStepping]. With no call to follow, [idle] alone.
 * With [bar] false the step shows as its title alone, for a place where a
 * spinner beside it already says it runs. The step's words take [textColor].
 */
@Composable
internal fun <Step : Any> AndroidProgress(
    steps: LatestSource<Step>?,
    title: (Step) -> String,
    fraction: (Step) -> Double?,
    modifier: Modifier = Modifier,
    bar: Boolean = true,
    textColor: Color = MaterialTheme.colorScheme.onSurface,
    onStepping: (Boolean) -> Unit = {},
    idle: @Composable () -> Unit,
) {
    var stepping by remember(steps) { mutableStateOf(false) }
    Column(modifier = modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        if (!stepping || steps == null) idle()
        if (steps != null) {
            val accent = MaterialTheme.colorScheme.primary.toArgb()
            val track =
                MaterialTheme.colorScheme.primary
                    .copy(alpha = ThemeOpacity.tintStrong)
                    .toArgb()
            val text = textColor.toArgb()
            val style = ThemeText.body.style
            AndroidView(
                modifier = Modifier.fillMaxWidth(),
                factory = { context -> StepProgressView<Step>(context) },
                update = { view ->
                    view.applyStyle(
                        accent = accent,
                        track = track,
                        text = text,
                        textSizeSp = style.fontSize.value,
                        lineHeightSp = style.lineHeight.value,
                    )
                    view.describe(title, fraction)
                    view.showsBar = bar
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

/** A library open's [steps], each named as core words its key, over a bar of its share done; the rest as above. */
@Composable
internal fun AndroidProgress(
    steps: LatestSource<BridgeOpenProgress>?,
    modifier: Modifier = Modifier,
    bar: Boolean = true,
    textColor: Color = MaterialTheme.colorScheme.onSurface,
    onStepping: (Boolean) -> Unit = {},
    idle: @Composable () -> Unit,
) {
    val context = LocalContext.current
    AndroidProgress(
        steps = steps,
        title = { context.coreString(it.key) },
        fraction = { it.fraction },
        modifier = modifier,
        bar = bar,
        textColor = textColor,
        onStepping = onStepping,
        idle = idle,
    )
}

/**
 * What shows while a call runs that has no steps to follow: [title], the words for what it does, over a moving bar
 * that screen readers name by [title], as a step that counts nothing shows.
 */
@Composable
internal fun AndroidProgress(
    title: String,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        Text(title, style = ThemeText.body.style, color = MaterialTheme.colorScheme.onSurface)
        MovingBar(description = title)
    }
}

/** A call's step over its bar, gone between steps. */
internal class StepProgressView<Step : Any>(
    context: Context,
) : LinearLayout(context) {
    private val label = TextView(context).apply { includeFontPadding = false }
    private val line = FrameLayout(context)
    private val bar = ShareBarView(context)
    private val following = FrameFollowing<Step>(this, ::show)
    private lateinit var title: (Step) -> String
    private lateinit var fraction: (Step) -> Double?

    /** Whether a step runs, as last told to [onStepping]; null until the View shows what its stream holds. */
    private var told: Boolean? = null
    var onStepping: (Boolean) -> Unit = {}

    /** Whether the bar shows under the step's title. */
    var showsBar: Boolean = true
        set(value) {
            field = value
            bar.visibility = if (value) View.VISIBLE else View.GONE
        }

    init {
        orientation = VERTICAL
        layoutParams = LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        line.addView(
            label,
            FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
                Gravity.CENTER_VERTICAL,
            ),
        )
        addView(line, LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        val gap =
            TypedValue.applyDimension(
                TypedValue.COMPLEX_UNIT_DIP,
                ThemeSpace.related.value,
                resources.displayMetrics,
            )
        addView(
            bar,
            LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                topMargin = gap.toInt()
            },
        )
        visibility = View.GONE
    }

    fun applyStyle(
        accent: Int,
        track: Int,
        text: Int,
        textSizeSp: Float,
        lineHeightSp: Float,
    ) {
        bar.applyColors(accent = accent, track = track)
        label.setTextColor(text)
        label.setTextSize(TypedValue.COMPLEX_UNIT_SP, textSizeSp)
        // The words sit centred in a line box of the style's line height, as Text draws them, and the box is what
        // spaces the bar below.
        line.minimumHeight =
            TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, lineHeightSp, resources.displayMetrics).toInt()
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
            if (label.text.toString() != shown) {
                label.text = shown
                bar.contentDescription = shown
            }
            val done = fraction(step)
            if (done != null) bar.showFraction(done.toFloat()) else bar.showMoving()
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
