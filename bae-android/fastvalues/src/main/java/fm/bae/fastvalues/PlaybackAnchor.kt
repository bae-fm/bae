package fm.bae.fastvalues

import uniffi.bae_bridge.BridgeTrackPosition

// What core puts on a position's stream is an anchor, a position at a moment, and the rate it moves at from
// there. A View works out where playback has got to each display frame from the anchor and the frame's time,
// so nothing is sent as playback plays.

/**
 * The clock display frames are timed on, which core stamps anchors with: `CLOCK_MONOTONIC`, as
 * `Choreographer`'s frame times and [System.nanoTime] are.
 */
object FrameClock {
    fun nowNanos(): Long = System.nanoTime()
}

/**
 * The position at a frame timed [frameTimeNanos] on the frame clock: the anchor's plus the time since it times the
 * rate, held to the track's length. A frame timed before the anchor shows the anchor.
 */
fun BridgeTrackPosition.positionAt(frameTimeNanos: Long): Long {
    val frame = frameTimeNanos.toULong()
    val elapsedNanos = if (frame > anchoredAtNs) (frame - anchoredAtNs).toDouble() else 0.0
    val position = positionMs.toDouble() + elapsedNanos / NANOS_PER_MILLI * rate
    val held = durationMs?.let { minOf(position, it.toDouble()) } ?: position
    return Math.floor(held).toLong()
}

/** The seek bar's fill at a frame timed [frameTimeNanos]: zero throughout when the track's length is unknown. */
fun BridgeTrackPosition.progressAt(frameTimeNanos: Long): Double {
    val duration = durationMs?.takeIf { it > 0uL } ?: return 0.0
    return (positionAt(frameTimeNanos).toDouble() / duration.toDouble()).coerceIn(0.0, 1.0)
}

private const val NANOS_PER_MILLI = 1_000_000.0
