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
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import androidx.media3.common.C
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.PlaybackPosition
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch

private const val SEEK_BAR_MAX = 10_000
private const val TIME_LABEL_TEXT_SIZE_SP = 11f
private const val TIME_LABEL_WIDTH_DP = 48

@Composable
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
fun PlaybackProgressAndroidView(
    player: BaeCorePlayer,
    modifier: Modifier = Modifier,
) {
    AndroidView(
        modifier = modifier,
        factory = { context -> PlaybackProgressView(context) },
        update = { view ->
            view.bind(
                position = player.position,
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
    private var position: Flow<PlaybackPosition>? = null
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

    fun setPosition(position: PlaybackPosition) {
        elapsedTextView.text = position.elapsedLabel
        remainingTextView.text = position.remainingLabel
        if (!isDragging) {
            seekBar.progress = progressToSeekBar(position.progress)
        }
    }

    fun bind(
        position: Flow<PlaybackPosition>,
        onSeekRatio: (Double) -> Unit,
    ) {
        this.onSeekRatio = onSeekRatio
        if (this.position === position && positionJob?.isActive == true) {
            return
        }
        this.position = position
        positionScope?.cancel()
        val scope = CoroutineScope(Dispatchers.Main.immediate + SupervisorJob())
        positionScope = scope
        positionJob =
            scope.launch {
                position.collect { setPosition(it) }
            }
    }

    override fun onDetachedFromWindow() {
        positionScope?.cancel()
        positionScope = null
        positionJob = null
        position = null
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
