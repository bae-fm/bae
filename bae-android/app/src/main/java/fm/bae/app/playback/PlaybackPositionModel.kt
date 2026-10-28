package fm.bae.app.playback

/**
 * Pure seek and position math for the Media3 playback state. Owns the position
 * anchor from the lines core's timeline tells, the current track's duration,
 * and the pending in-track seek whose dropped position wins until core
 * confirms it.
 *
 * Holds no Media3 or bridge types: track ids are plain strings, positions and
 * durations are milliseconds, and an unknown duration is null. [BaeCorePlayer]
 * holds one instance, feeds it the lines, and reads its outputs for the Media3
 * state.
 */
internal class PlaybackPositionModel {
    /** Position anchor from the latest progress event. */
    private var anchorPositionMs: Long = 0L

    /**
     * Whether the line the anchor came from moves. Through a pregap it stands
     * at the track's start while the track plays, and Media3's position
     * stands with it.
     */
    var lineMoves: Boolean = true
        private set

    /** The current track's duration, or null when its length is unknown. */
    var durationMs: Long? = null
        private set

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
     * Fold a line into the position state. A pending in-track seek for
     * the incoming track outranks live progress ([PositionUpdate.HeldByPendingSeek])
     * until [onSeeked] confirms it; a position for a different track than the one
     * playing is rejected ([PositionUpdate.StaleTrack]); otherwise the anchor
     * and duration advance ([PositionUpdate.Applied]).
     */
    fun onProgress(
        currentTrackId: String?,
        trackId: String,
        positionMs: Long,
        durationMs: Long?,
        moving: Boolean,
    ): PositionUpdate {
        val pendingSeek = pendingSeek
        if (pendingSeek != null && pendingSeek.matches(trackId)) {
            return PositionUpdate.HeldByPendingSeek
        }
        return apply(currentTrackId, trackId, positionMs, durationMs, moving)
    }

    /**
     * Fold a seek's line into the position state, clearing the pending
     * seek it confirms. A position for a different track than the one playing is
     * rejected ([PositionUpdate.StaleTrack]).
     */
    fun onSeeked(
        currentTrackId: String?,
        trackId: String,
        positionMs: Long,
        durationMs: Long?,
        moving: Boolean,
    ): PositionUpdate {
        if (!targetsCurrentTrack(currentTrackId, trackId)) {
            return PositionUpdate.StaleTrack
        }
        pendingSeek = null
        return apply(currentTrackId, trackId, positionMs, durationMs, moving)
    }

    private fun apply(
        currentTrackId: String?,
        trackId: String,
        positionMs: Long,
        durationMs: Long?,
        moving: Boolean,
    ): PositionUpdate {
        if (!targetsCurrentTrack(currentTrackId, trackId)) {
            return PositionUpdate.StaleTrack
        }
        anchorPositionMs = positionMs
        this.durationMs = durationMs
        lineMoves = moving
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
     * ([durationMs] is null when the length is unknown). Leaves the anchor to
     * the lines.
     */
    fun setActiveTrack(
        trackChanged: Boolean,
        durationMs: Long?,
    ) {
        if (trackChanged) {
            pendingSeek = null
        }
        this.durationMs = durationMs
    }

    /**
     * Begin an in-track seek to [requestedPositionMs], projecting the dropped
     * position (clamped into the track) until core confirms it. Returns
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
        lineMoves = true
    }
}

/** The outcome of folding a line or a seek's line into [PlaybackPositionModel]. */
internal enum class PositionUpdate {
    /** The position advanced; the player should republish its projection. */
    Applied,

    /** A pending in-track seek for the current track outranks this line until
     *  core confirms the seek — the projection holds, nothing republishes. */
    HeldByPendingSeek,

    /** The position is for a track that is not the one playing. */
    StaleTrack,
}
