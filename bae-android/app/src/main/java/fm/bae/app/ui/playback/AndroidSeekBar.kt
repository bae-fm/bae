package fm.bae.app.ui.playback

import android.content.Context
import android.graphics.Typeface
import android.util.TypedValue
import android.view.Gravity
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.SeekBar
import android.widget.TextView
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.ViewCompat
import androidx.media3.common.C
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.clockText
import fm.bae.app.currentLocale
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.fastvalues.FrameClock
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.positionAt
import fm.bae.fastvalues.progressAt
import uniffi.bae_bridge.BridgeTrackBuffered
import uniffi.bae_bridge.BridgeTrackPosition
import uniffi.bae_bridge.bridgeSeekBar

private const val SEEK_BAR_MAX = 10_000
private const val TIME_LABEL_TEXT_SIZE_SP = 11f

/** The space between a time label and the bar. */
private const val TIME_LABEL_GAP_DP = 8

/** The space above the row, which both looks keep, in dp. */
private const val ROW_TOP_DP = 4

/** What the seek bar draws: the [0,1] slider fraction and the two time labels core
 *  picks, and whether it takes a seek — not while the track's length is unknown,
 *  when no fill points at a position. */
internal data class SeekBarState(
    val progress: Double,
    val leading: String,
    val trailing: String,
    val seekable: Boolean,
)

/**
 * The bar's state for [position] at a frame timed [frameTimeNanos] on the frame clock, with empty
 * labels when nothing is playing.
 */
internal fun Context.seekBarState(
    position: BridgeTrackPosition?,
    frameTimeNanos: Long,
    showRemaining: Boolean,
): SeekBarState {
    position ?: return SeekBarState(0.0, "", "", seekable = false)
    val clocks = bridgeSeekBar(position.positionAt(frameTimeNanos), position.durationMs, showRemaining)
    return SeekBarState(
        progress = position.progressAt(frameTimeNanos),
        leading = clockText(clocks.leading, currentLocale()),
        trailing = clocks.trailing?.let { clockText(it, currentLocale()) } ?: "",
        seekable = position.durationMs != null,
    )
}

@Composable
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
fun AndroidSeekBarContainer(
    session: OpenLibrary,
    player: BaeCorePlayer,
    modifier: Modifier = Modifier,
    /**
     * Runs the person's toggle of the time label and shows its failure: a
     * snackbar by default, or inline where a sheet would cover one.
     */
    perform: (operation: String, action: suspend () -> Unit) -> Unit = session.messages::perform,
    look: SeekLineLook = SeekLineLook.BAR,
) {
    val context = LocalContext.current
    val config by session.configStore.config.collectAsState()
    val showRemaining = config.showRemainingTime
    val nowPlaying by player.nowPlaying.collectAsState()
    AndroidSeekBar(
        position = nowPlaying?.let { player.trackPositions.source(it.trackId) },
        // The large line shows no held parts, so it follows none.
        buffered = if (look == SeekLineLook.BAR) nowPlaying?.let { player.trackBuffered.source(it.trackId) } else null,
        state = { position, frameTimeNanos -> context.seekBarState(position, frameTimeNanos, showRemaining) },
        onSeekRatio = { ratio ->
            val duration = player.duration
            if (duration != C.TIME_UNSET && duration > 0L) {
                player.seekTo((ratio * duration).toLong())
            }
        },
        // The config subscription re-renders the bar.
        onToggleRemaining = {
            perform("update show-remaining-time setting") {
                session.appHandle.setShowRemainingTime(!showRemaining)
            }
        },
        modifier = modifier,
        look = look,
    )
}

/**
 * The seek row: the time labels [state] words, and the bar, which draws
 * [position] over the parts of its track [buffered] holds, in the [look]
 * given. The View reads both streams each display frame, and works the position
 * out from the position's anchor and the frame's time.
 */
@Composable
internal fun AndroidSeekBar(
    position: LatestSource<BridgeTrackPosition>?,
    buffered: LatestSource<BridgeTrackBuffered>?,
    state: (BridgeTrackPosition?, Long) -> SeekBarState,
    onSeekRatio: (Double) -> Unit,
    onToggleRemaining: () -> Unit,
    modifier: Modifier = Modifier,
    look: SeekLineLook = SeekLineLook.BAR,
) {
    val colors = MaterialTheme.colorScheme
    val hairline = BaeTheme.colors.hairline
    val tintStrong = colors.primary.copy(alpha = ThemeOpacity.tintStrong)
    AndroidView(
        modifier = modifier.fillMaxWidth().seekRowLayout(look),
        factory = { viewContext -> PlaybackProgressView(viewContext, look) },
        update = { view ->
            view.applyColors(
                accent = colors.primary.toArgb(),
                held = tintStrong.toArgb(),
                track = (if (look == SeekLineLook.LARGE) tintStrong else hairline).toArgb(),
                text = colors.onSurfaceVariant.toArgb(),
            )
            view.bind(
                position = position,
                buffered = buffered,
                state = state,
                onSeekRatio = onSeekRatio,
                onToggleRemaining = onToggleRemaining,
            )
        },
    )
}

/**
 * The row takes the space the design draws, [ROW_TOP_DP] above a line [SeekLineLook.rowHeightDp]
 * tall, while its view is [SeekLineLook.touchHeightDp] tall and centred on the line, so a touch
 * a little off the line still lands on the bar.
 */
private fun Modifier.seekRowLayout(look: SeekLineLook): Modifier =
    layout { measurable, constraints ->
        val viewHeight = look.touchHeightDp.dp.roundToPx()
        val placeable = measurable.measure(constraints.copy(minHeight = viewHeight, maxHeight = viewHeight))
        val rowHeight = (ROW_TOP_DP + look.rowHeightDp).dp.roundToPx()
        layout(placeable.width, rowHeight) {
            placeable.place(0, ROW_TOP_DP.dp.roundToPx() + (look.rowHeightDp.dp.roundToPx() - viewHeight) / 2)
        }
    }

private fun TextView.showText(text: String) {
    if (this.text.toString() != text) {
        this.text = text
    }
}

internal class PlaybackProgressView(
    context: Context,
    look: SeekLineLook = SeekLineLook.BAR,
) : LinearLayout(context) {
    internal val leadingTextView: TextView = timeLabel()

    private val line = SeekLineDrawable(resources.displayMetrics.density, look)

    /** The parts of the track whose bytes are here, drawn under the played part. */
    internal val heldRanges: HeldRangesDrawable = line.heldRanges
    internal val seekBar: SeekBar =
        SeekBar(context).apply {
            max = SEEK_BAR_MAX
            contentDescription = context.getString(R.string.seek_position)
            // The line draws its own thumb; the bar draws only the line.
            thumb = null
            progressDrawable = line
            setOnSeekBarChangeListener(
                object : SeekBar.OnSeekBarChangeListener {
                    override fun onProgressChanged(
                        seekBar: SeekBar,
                        progress: Int,
                        fromUser: Boolean,
                    ) {
                        if (isDragging) showShown()
                    }

                    override fun onStartTrackingTouch(seekBar: SeekBar) {
                        beginTracking()
                    }

                    override fun onStopTrackingTouch(seekBar: SeekBar) {
                        finishTracking()
                    }
                },
            )
        }
    internal val trailingTextView: TextView = timeLabel()

    var onSeekRatio: (Double) -> Unit = {}
    var onToggleRemaining: () -> Unit = {}

    private var isDragging = false
    private val following =
        FrameFollowing<BridgeTrackPosition>(
            this,
            show = { show(it) },
            onFrame = { _, frameTimeNanos -> showShown(frameTimeNanos) },
        )
    private val bufferedFollowing =
        FrameFollowing<BridgeTrackBuffered>(this, show = { heldRanges.show(it?.ranges.orEmpty()) })

    /** What the bar draws of a position at a frame's time; set by [bind]. */
    private var state: (BridgeTrackPosition?, Long) -> SeekBarState =
        { _, _ -> SeekBarState(0.0, "", "", seekable = false) }

    /** What the stream last said; null while it holds nothing. */
    private var position: BridgeTrackPosition? = null

    /** The seek the person made, shown until a position of a later applied seek arrives. */
    private var projected: BridgeTrackPosition? = null

    init {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        layoutParams =
            LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            )
        // Tapping the leading label switches it between elapsed and remaining.
        leadingTextView.setOnClickListener { onToggleRemaining() }
        addView(leadingTextView)
        // The line runs to the labels, a gap from them: only the thumb's half, which it draws
        // past either end of the line, pads the bar, and the gap is what that leaves.
        val thumbHalf = Math.ceil((look.thumbHalfWidthDp * resources.displayMetrics.density).toDouble()).toInt()
        seekBar.setPaddingRelative(thumbHalf, 0, thumbHalf, 0)
        addView(
            seekBar,
            LayoutParams(0, dp(look.touchHeightDp.toInt()), 1f).apply {
                marginStart = dp(TIME_LABEL_GAP_DP) - thumbHalf
                marginEnd = dp(TIME_LABEL_GAP_DP) - thumbHalf
            },
        )
        addView(trailingTextView)
    }

    fun applyColors(
        accent: Int,
        held: Int,
        track: Int,
        text: Int,
    ) {
        line.accent = accent
        heldRanges.color = held
        line.track = track
        leadingTextView.setTextColor(text)
        trailingTextView.setTextColor(text)
    }

    // A label is set at every position tick and its text changes at most once
    // a second, so it is written only when it changes.
    fun setPosition(state: SeekBarState) {
        leadingTextView.showText(state.leading)
        trailingTextView.showText(state.trailing)
        // The bar's value for a screen reader: the clocks it shows beside it.
        val spoken = listOf(state.leading, state.trailing).filter { it.isNotEmpty() }.joinToString(", ")
        if (ViewCompat.getStateDescription(seekBar)?.toString() != spoken) {
            ViewCompat.setStateDescription(seekBar, spoken)
        }
        seekBar.isEnabled = state.seekable
        if (!isDragging) {
            seekBar.progress = progressToSeekBar(state.progress)
        }
    }

    /**
     * Draw [position] and the parts of its track [buffered] holds from now
     * on, or nothing, through [state]. The same streams already followed
     * change nothing.
     */
    fun bind(
        position: LatestSource<BridgeTrackPosition>?,
        buffered: LatestSource<BridgeTrackBuffered>?,
        state: (BridgeTrackPosition?, Long) -> SeekBarState,
        onSeekRatio: (Double) -> Unit,
        onToggleRemaining: () -> Unit,
    ) {
        this.onSeekRatio = onSeekRatio
        this.onToggleRemaining = onToggleRemaining
        this.state = state
        if (position == null) {
            following.stop()
            projected = null
            show(null)
        } else {
            // A seek made on another track is not this one's.
            if (!following.follows(position)) projected = null
            following.follow(position)
            showShown()
        }
        if (buffered == null) {
            bufferedFollowing.stop()
            heldRanges.show(emptyList())
        } else {
            bufferedFollowing.follow(buffered)
        }
    }

    /** A position the stream holds; a seek the person made shows until one of a later applied seek. */
    private fun show(position: BridgeTrackPosition?) {
        this.position = position
        val projected = projected
        if (projected != null && position?.seekRevision != projected.seekRevision) this.projected = null
        showShown()
    }

    /**
     * What the labels and fill show: the dragged position while the person drags, else the
     * seek they dropped until core applies it, else the stream's position.
     */
    private fun showShown(frameTimeNanos: Long = FrameClock.nowNanos()) {
        setPosition(
            state(if (isDragging) positionAt(draggedRatio()) ?: position else projected ?: position, frameTimeNanos),
        )
    }

    private fun draggedRatio(): Double = seekBar.progress.toDouble() / seekBar.max.toDouble()

    /** The stream's position standing at [ratio] of its track; null while the length is unknown. */
    private fun positionAt(ratio: Double): BridgeTrackPosition? {
        val position = position
        val durationMs = position?.durationMs ?: return null
        return position.copy(positionMs = (ratio * durationMs.toDouble()).toLong(), rate = 0.0, anchoredAtNs = 0uL)
    }

    internal fun beginTracking() {
        isDragging = true
    }

    internal fun finishTracking() {
        val ratio = draggedRatio()
        isDragging = false
        projected = positionAt(ratio) ?: projected
        showShown()
        onSeekRatio(ratio)
    }

    /** A time label in the body face, its digits all one width so the clock doesn't jostle as it ticks. */
    private fun timeLabel(): TextView =
        TextView(context).apply {
            typeface = Typeface.DEFAULT
            fontFeatureSettings = "tnum"
            setTextSize(TypedValue.COMPLEX_UNIT_SP, TIME_LABEL_TEXT_SIZE_SP)
            layoutParams =
                LayoutParams(
                    ViewGroup.LayoutParams.WRAP_CONTENT,
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
}
