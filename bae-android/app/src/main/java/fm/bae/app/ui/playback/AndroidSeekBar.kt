package fm.bae.app.ui.playback

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Typeface
import android.graphics.drawable.ClipDrawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.LayerDrawable
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
import androidx.compose.ui.platform.LocalContext
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
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeTrackBuffered
import uniffi.bae_bridge.BridgeTrackPosition
import uniffi.bae_bridge.bridgeSeekBar

private const val SEEK_BAR_MAX = 10_000
private const val TIME_LABEL_TEXT_SIZE_SP = 11f

/** The space between a time label and the bar. */
private const val TIME_LABEL_GAP_DP = 8
private const val TRACK_HEIGHT_DP = 4

/** What the seek bar draws: the [0,1] slider fraction and the two time labels core
 *  picks, and whether it takes a seek — not while the track's length is unknown,
 *  when no fill points at a position. */
internal data class SeekBarState(
    val progress: Double,
    val leading: String,
    val trailing: String,
    val seekable: Boolean,
)

/** The bar's state for [position], with empty labels when nothing is playing. */
internal fun Context.seekBarState(
    position: BridgeTrackPosition?,
    showRemaining: Boolean,
): SeekBarState {
    position ?: return SeekBarState(0.0, "", "", seekable = false)
    val clocks = bridgeSeekBar(position.positionMs, position.durationMs, showRemaining)
    return SeekBarState(
        progress = position.progress,
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
) {
    val context = LocalContext.current
    val config by session.configStore.config.collectAsState()
    val showRemaining = config.showRemainingTime
    val nowPlaying by player.nowPlaying.collectAsState()
    AndroidSeekBar(
        position = nowPlaying?.let { player.trackPositions.source(it.trackId) },
        buffered = nowPlaying?.let { player.trackBuffered.source(it.trackId) },
        state = { context.seekBarState(it, showRemaining) },
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
    )
}

/**
 * The seek row: the time labels [state] words, and the bar, which draws
 * [position] over the parts of its track [buffered] holds. The View reads
 * both streams each display frame.
 */
@Composable
internal fun AndroidSeekBar(
    position: LatestSource<BridgeTrackPosition>?,
    buffered: LatestSource<BridgeTrackBuffered>?,
    state: (BridgeTrackPosition?) -> SeekBarState,
    onSeekRatio: (Double) -> Unit,
    onToggleRemaining: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    val hairline = BaeTheme.colors.hairline
    AndroidView(
        modifier = modifier.fillMaxWidth(),
        factory = { viewContext -> PlaybackProgressView(viewContext) },
        update = { view ->
            view.applyColors(
                accent = colors.primary.toArgb(),
                held = colors.primary.copy(alpha = ThemeOpacity.tintStrong).toArgb(),
                track = hairline.toArgb(),
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

private fun TextView.showText(text: String) {
    if (this.text.toString() != text) {
        this.text = text
    }
}

internal class PlaybackProgressView(
    context: Context,
) : LinearLayout(context) {
    internal val leadingTextView: TextView = timeLabel()

    /** The parts of the track whose bytes are here, drawn under the played part. */
    internal val heldRanges = HeldRangesDrawable()
    private val trackLine = GradientDrawable()
    private val playedLine = GradientDrawable()
    internal val seekBar: SeekBar =
        SeekBar(context).apply {
            max = SEEK_BAR_MAX
            contentDescription = context.getString(R.string.seek_position)
            progressDrawable = trackLayers()
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
    internal val trailingTextView: TextView = timeLabel()

    var onSeekRatio: (Double) -> Unit = {}
    var onToggleRemaining: () -> Unit = {}

    private var isDragging = false
    private val following = FrameFollowing<BridgeTrackPosition>(this) { show(it) }
    private val bufferedFollowing = FrameFollowing<BridgeTrackBuffered>(this) { heldRanges.show(it?.ranges.orEmpty()) }

    /** What the bar draws of a position; set by [bind]. */
    private var state: (BridgeTrackPosition?) -> SeekBarState = { SeekBarState(0.0, "", "", seekable = false) }

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
        addView(
            seekBar,
            LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply {
                marginStart = dp(TIME_LABEL_GAP_DP)
                marginEnd = dp(TIME_LABEL_GAP_DP)
            },
        )
        addView(trailingTextView)
        // The bar runs to the labels: only the thumb's half, which it draws
        // past either end of the line, pads it.
        val thumbHalf = seekBar.thumb.intrinsicWidth / 2
        seekBar.setPaddingRelative(thumbHalf, seekBar.paddingTop, thumbHalf, seekBar.paddingBottom)
    }

    /** The bar's line: the track, the parts held here over it, and the played part over those. */
    private fun trackLayers(): LayerDrawable {
        val corner = dp(TRACK_HEIGHT_DP) / 2f
        trackLine.cornerRadius = corner
        playedLine.cornerRadius = corner
        val played = ClipDrawable(playedLine, Gravity.START, ClipDrawable.HORIZONTAL)
        return LayerDrawable(arrayOf(trackLine, heldRanges, played)).apply {
            setId(0, android.R.id.background)
            setId(2, android.R.id.progress)
            for (layer in 0 until numberOfLayers) {
                setLayerGravity(layer, Gravity.CENTER_VERTICAL or Gravity.FILL_HORIZONTAL)
                setLayerHeight(layer, dp(TRACK_HEIGHT_DP))
            }
        }
    }

    fun applyColors(
        accent: Int,
        held: Int,
        track: Int,
        text: Int,
    ) {
        playedLine.setColor(accent)
        heldRanges.color = held
        trackLine.setColor(track)
        seekBar.thumbTintList = ColorStateList.valueOf(accent)
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
        state: (BridgeTrackPosition?) -> SeekBarState,
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

    private fun showShown() {
        setPosition(state(projected ?: position))
    }

    internal fun beginTracking() {
        isDragging = true
    }

    internal fun finishTracking() {
        val ratio = seekBar.progress.toDouble() / seekBar.max.toDouble()
        isDragging = false
        val position = position
        val durationMs = position?.durationMs
        if (position != null && durationMs != null) {
            projected =
                position.copy(
                    positionMs = (ratio * durationMs.toDouble()).toLong(),
                    progress = ratio,
                )
            showShown()
        }
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
