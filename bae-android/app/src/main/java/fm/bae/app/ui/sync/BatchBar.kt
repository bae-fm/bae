package fm.bae.app.ui.sync

import android.content.Context
import android.graphics.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestStream
import uniffi.bae_bridge.BridgeBatchBar

/**
 * A transfer line's bar: the share of its batch's bytes done, as the line's row says, then for
 * each file or release on the move a segment that starts where the row says and spans its share,
 * filled to the fraction [stream] holds under its key. Filled in the accent over a track of the
 * accent's tint. Screen readers name it [description], the line's words. Each segment reads its
 * key's latest value each display frame while its bytes move: arriving bytes recompose nothing.
 */
@Composable
internal fun <V : Any> BatchBar(
    bar: BridgeBatchBar,
    stream: LatestStream<V>,
    fraction: (V) -> Double,
    description: String,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val track = accent.copy(alpha = ThemeOpacity.tintStrong)
    AndroidView(
        modifier = modifier.fillMaxWidth(),
        factory = { context -> BatchBarView<V>(context) },
        update = { view ->
            view.contentDescription = description
            view.applyColors(accent = accent.toArgb(), track = track.toArgb())
            view.show(bar, stream, fraction)
        },
        onRelease = { view -> view.stopFollowing() },
    )
}

/**
 * Draws a [BatchBar]: four tall and rounded, the track, the bytes done from its start, and each
 * segment filled from where it starts. Each segment's fraction is read by its own
 * [FrameFollowing], one per place in the row's list, so a file that follows another in its place
 * reuses its reader.
 */
internal class BatchBarView<V : Any>(
    context: Context,
) : TrackBarView(context) {
    /** One file or release on the move, and how far its stream says it has got. */
    private class Segment<V : Any>(
        val following: FrameFollowing<V>,
    ) {
        var start = 0f
        var share = 0f
        var fraction = 0f
    }

    private var done = 0f
    override val shareDone: Float get() = done
    private val segments = mutableListOf<Segment<V>>()
    private var shown = 0
    private var fraction: (V) -> Double = { 0.0 }

    /** Draw [bar], each segment following its key's value in [stream]. */
    fun show(
        bar: BridgeBatchBar,
        stream: LatestStream<V>,
        fraction: (V) -> Double,
    ) {
        this.fraction = fraction
        done = bar.done.toFloat()
        bar.moving.forEachIndexed { index, moving ->
            val segment =
                segments.getOrNull(index) ?: Segment<V>(FrameFollowing(this) { value -> show(index, value) }).also {
                    segments += it
                }
            segment.start = moving.start.toFloat()
            segment.share = moving.share.toFloat()
            segment.following.follow(stream.source(moving.key))
        }
        for (index in bar.moving.size until shown) {
            segments[index].following.stop()
            segments[index].fraction = 0f
        }
        shown = bar.moving.size
        invalidate()
    }

    fun stopFollowing() {
        segments.forEach { it.following.stop() }
        shown = 0
    }

    private fun show(
        index: Int,
        value: V?,
    ) {
        segments[index].fraction = (value?.let(fraction) ?: 0.0).coerceIn(0.0, 1.0).toFloat()
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        val height = barHeight()
        val radius = height / 2f
        val width = width.toFloat()
        canvas.drawRoundRect(bounds.apply { set(0f, 0f, width, height) }, radius, radius, track)
        if (done > 0f) {
            canvas.drawRoundRect(bounds.apply { set(0f, 0f, width * done, height) }, radius, radius, fill)
        }
        for (segment in segments.take(shown)) {
            if (segment.fraction <= 0f) continue
            val start = width * segment.start
            val end = width * (segment.start + segment.share * segment.fraction)
            canvas.drawRoundRect(bounds.apply { set(start, 0f, end, height) }, radius, radius, fill)
        }
    }
}
