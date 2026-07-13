package fm.bae.app.ui

import android.content.Context
import android.graphics.Typeface
import android.util.TypedValue
import android.view.Gravity
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.SeekBar
import android.widget.TextView
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.media3.common.C
import fm.bae.app.durationClockText
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.PlaybackPosition
import fm.bae.app.remainingClockText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

private const val SEEK_BAR_MAX = 10_000
private const val TIME_LABEL_TEXT_SIZE_SP = 11f
private const val TIME_LABEL_WIDTH_DP = 48

/** What the seek bar draws: the [0,1] slider fraction and the two clock labels
 *  rendered from core's projection of the raw position. */
internal data class SeekBarState(
    val progress: Double,
    val elapsed: String,
    val remaining: String,
)

/** Render a raw playback position into the labels the bar draws. A track with no
 *  known length has no countdown to show. */
internal fun Context.seekBarState(position: PlaybackPosition): SeekBarState =
    SeekBarState(
        progress = position.progress,
        elapsed = durationClockText(position.positionMs),
        remaining =
            if (position.positionMs != null && position.durationMs != null) {
                remainingClockText(position.positionMs, position.durationMs)
            } else {
                ""
            },
    )

@Composable
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
fun PlaybackProgressAndroidView(
    player: BaeCorePlayer,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    // One flow instance per (player, context): the view rebinds only when the
    // flow identity changes, so mapping inline would re-collect every recomposition.
    val state = remember(player, context) { player.position.map(context::seekBarState) }
    AndroidView(
        modifier = modifier,
        factory = { viewContext -> PlaybackProgressView(viewContext) },
        update = { view ->
            view.bind(
                state = state,
                onSeekRatio = { ratio ->
                    val duration = player.duration
                    if (duration != C.TIME_UNSET && duration > 0L) {
                        player.seekTo((ratio * duration).toLong())
                    }
                },
            )
        },
    )
}

internal class PlaybackProgressView(
    context: Context,
) : LinearLayout(context) {
    internal val elapsedTextView: TextView = timeLabel()
    internal val seekBar: SeekBar =
        SeekBar(context).apply {
            max = SEEK_BAR_MAX
            setOnSeekBarChangeListener(
                object : SeekBar.OnSeekBarChangeListener {
                    override fun onProgressChanged(
                        seekBar: SeekBar,
                        progress: Int,
                        fromUser: Boolean,
                    ) = Unit

                    override fun onStartTrackingTouch(seekBar: SeekBar) {
                        beginTracking()
                    }

                    override fun onStopTrackingTouch(seekBar: SeekBar) {
                        finishTracking()
                    }
                },
            )
        }
    internal val remainingTextView: TextView = timeLabel()

    var onSeekRatio: (Double) -> Unit = {}

    private var isDragging = false
    private var state: Flow<SeekBarState>? = null
    private var positionScope: CoroutineScope? = null
    private var positionJob: Job? = null

    init {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        layoutParams =
            LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            )
        addView(elapsedTextView)
        addView(
            seekBar,
            LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f),
        )
        addView(remainingTextView)
    }

    fun setPosition(state: SeekBarState) {
        elapsedTextView.text = state.elapsed
        remainingTextView.text = state.remaining
        if (!isDragging) {
            seekBar.progress = progressToSeekBar(state.progress)
        }
    }

    fun bind(
        state: Flow<SeekBarState>,
        onSeekRatio: (Double) -> Unit,
    ) {
        this.onSeekRatio = onSeekRatio
        if (this.state === state && positionJob?.isActive == true) {
            return
        }
        this.state = state
        positionScope?.cancel()
        val scope = CoroutineScope(Dispatchers.Main.immediate + SupervisorJob())
        positionScope = scope
        positionJob =
            scope.launch {
                state.collect { setPosition(it) }
            }
    }

    override fun onDetachedFromWindow() {
        positionScope?.cancel()
        positionScope = null
        positionJob = null
        state = null
        super.onDetachedFromWindow()
    }

    internal fun beginTracking() {
        isDragging = true
    }

    internal fun finishTracking() {
        val ratio = seekBar.progress.toDouble() / seekBar.max.toDouble()
        isDragging = false
        onSeekRatio(ratio)
    }

    private fun timeLabel(): TextView =
        TextView(context).apply {
            typeface = Typeface.MONOSPACE
            setTextSize(TypedValue.COMPLEX_UNIT_SP, TIME_LABEL_TEXT_SIZE_SP)
            setTextColor(resolveThemeColor(android.R.attr.textColorSecondary))
            gravity = Gravity.CENTER
            layoutParams =
                LayoutParams(
                    dp(TIME_LABEL_WIDTH_DP),
                    ViewGroup.LayoutParams.WRAP_CONTENT,
                )
        }

    private fun progressToSeekBar(progress: Double): Int =
        progress
            .coerceIn(0.0, 1.0)
            .times(seekBar.max.toDouble())
            .toInt()

    private fun dp(value: Int): Int =
        TypedValue
            .applyDimension(
                TypedValue.COMPLEX_UNIT_DIP,
                value.toFloat(),
                resources.displayMetrics,
            ).toInt()

    private fun resolveThemeColor(attribute: Int): Int {
        val outValue = TypedValue()
        context.theme.resolveAttribute(attribute, outValue, true)
        return outValue.data
    }
}
