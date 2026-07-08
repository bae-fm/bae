package fm.bae.app.playback

import fm.bae.app.formatDurationMs
import fm.bae.app.formatRemainingMs

/**
 * Pure seek and position math for the now-playing bar and the Media3 playback
 * state. Owns the position anchor from core's progress events, the current
 * track's duration, the latest raw progress fraction, and the pending in-track
 * seek whose dropped position wins until core's progress catches up.
 *
 * Holds no Media3 or bridge types: track ids are plain strings, positions and
 * durations are milliseconds, and an unknown duration is null. [BaeCorePlayer]
 * holds one instance, feeds it the playback events, and reads its outputs for
 * both the Media3 state and the position StateFlow.
 */
internal class PlaybackPositionModel {
    /** Position anchor from the latest progress event. */
    private var anchorPositionMs: Long = 0L

    /** The current track's duration, or null when core has reported none. Non-null
     *  is always positive (a reported 0 means "unknown" and maps to null). */
    var durationMs: Long? = null
        private set

    /** Latest raw [0,1] progress fraction from core, used for the slider when no
     *  duration is known to derive it from the position. */
    private var progress: Double = 0.0

    private var pendingSeek: PendingSeek? = null

    /** The position to render: the pending seek's target while it holds, else the
     *  last anchor core reported. */
    val effectivePositionMs: Long
        get() = pendingSeek?.targetPositionMs ?: anchorPositionMs

    private data class PendingSeek(
        val trackId: String?,
        val targetPositionMs: Long,
    ) {
        fun matches(trackId: String): Boolean = this.trackId == null || this.trackId == trackId
    }

    /**
     * Fold a progress event into the position state. A pending in-track seek for
     * the incoming track outranks live progress ([PositionUpdate.HeldByPendingSeek])
     * until [onSeeked] confirms it; a position for a different track than the one
     * playing is rejected ([PositionUpdate.StaleTrack]); otherwise the anchor,
     * duration, and progress advance ([PositionUpdate.Applied]).
     */
    fun onProgress(
        currentTrackId: String?,
        trackId: String,
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    ): PositionUpdate {
        val pendingSeek = pendingSeek
        if (pendingSeek != null && pendingSeek.matches(trackId)) {
            return PositionUpdate.HeldByPendingSeek
        }
        return apply(currentTrackId, trackId, positionMs, durationMs, progress)
    }

    /**
     * Fold a seek-confirmation event into the position state, clearing the pending
     * seek it confirms. A position for a different track than the one playing is
     * rejected ([PositionUpdate.StaleTrack]).
     */
    fun onSeeked(
        currentTrackId: String?,
        trackId: String,
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    ): PositionUpdate {
        if (!targetsCurrentTrack(currentTrackId, trackId)) {
            return PositionUpdate.StaleTrack
        }
        pendingSeek = null
        return apply(currentTrackId, trackId, positionMs, durationMs, progress)
    }

    private fun apply(
        currentTrackId: String?,
        trackId: String,
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    ): PositionUpdate {
        if (!targetsCurrentTrack(currentTrackId, trackId)) {
            return PositionUpdate.StaleTrack
        }
        anchorPositionMs = positionMs
        this.durationMs = durationMs.takeIf { it > 0L }
        this.progress = progress
        return PositionUpdate.Applied
    }

    /** A position update targets the current track when no track is playing yet or
     *  it names the one that is; a position for a different track is stale. */
    private fun targetsCurrentTrack(
        currentTrackId: String?,
        trackId: String,
    ): Boolean = currentTrackId == null || currentTrackId == trackId

    /**
     * Enter or refresh the active track. Clears the pending seek when the track
     * changed (its projection no longer applies) and adopts the reported duration
     * ([rawDurationMs] of 0 means unknown). Leaves the anchor and progress to the
     * progress events.
     */
    fun setActiveTrack(
        trackChanged: Boolean,
        rawDurationMs: Long,
    ) {
        if (trackChanged) {
            pendingSeek = null
        }
        durationMs = rawDurationMs.takeIf { it > 0L }
    }

    /**
     * Begin an in-track seek to [requestedPositionMs], projecting the dropped
     * position (clamped into the track) until core's progress confirms it. Returns
     * the [0,1] ratio to send core, or null when there is no known duration to seek
     * within — the caller logs and ignores the seek.
     */
    fun beginInTrackSeek(
        trackId: String?,
        requestedPositionMs: Long,
    ): Double? {
        val total = durationMs ?: return null
        val targetPositionMs = requestedPositionMs.coerceIn(0L, total)
        pendingSeek = PendingSeek(trackId, targetPositionMs)
        return targetPositionMs.toDouble() / total.toDouble()
    }

    /** Drop the pending seek without confirming it — skipping to another queue
     *  entry abandons an in-track seek projected on the current one. */
    fun clearPendingSeek() {
        pendingSeek = null
    }

    /** Reset to the no-playback state. */
    fun reset() {
        pendingSeek = null
        anchorPositionMs = 0L
        durationMs = null
        progress = 0.0
    }

    /**
     * The seek-bar projection: the [0,1] slider fraction plus core's pre-formatted
     * elapsed/remaining labels. When a duration is known the fraction is derived
     * from the position; otherwise core's raw progress is used. Empty when no
     * track plays ([hasCurrentTrack] false).
     */
    fun position(hasCurrentTrack: Boolean): PlaybackPosition {
        if (!hasCurrentTrack) {
            return PlaybackPosition(0.0, "", "")
        }
        val positionMs = effectivePositionMs
        val total = durationMs
        val progress =
            if (total != null) {
                (positionMs.toDouble() / total.toDouble()).coerceIn(0.0, 1.0)
            } else {
                this.progress.coerceIn(0.0, 1.0)
            }
        return PlaybackPosition(
            progress = progress,
            elapsedLabel = formatDurationMs(positionMs),
            remainingLabel = if (total != null) formatRemainingMs(positionMs, total) else "",
        )
    }
}

/** The outcome of folding a progress or seek position into [PlaybackPositionModel]. */
internal enum class PositionUpdate {
    /** The position advanced; the player should republish its projection. */
    Applied,

    /** A pending in-track seek for the current track outranks this live progress
     *  until core confirms the seek — the projection holds, nothing republishes. */
    HeldByPendingSeek,

    /** The position is for a track that is not the one playing. */
    StaleTrack,
}
