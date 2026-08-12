package fm.bae.app.playback

import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLoadingTrackInfo
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeSidePausePrompt
import uniffi.bae_bridge.BridgeUiEvent

data class NowPlaying(
    val trackId: String,
    val title: String,
    val artist: String,
    /** The cover the bar fetches bytes for, or null when there is none. */
    val coverImage: BridgeImageRef?,
    val sidePausePrompt: BridgeSidePausePrompt?,
)

/**
 * Live playback position for the seek bar. [progress] is the bridge's [0,1]
 * fraction for the slider; [positionMs] and [durationMs] are raw milliseconds,
 * which the seek bar turns into clock labels through core's projection. Null
 * means there is no label: nothing is playing, or the track's length is unknown.
 */
data class PlaybackPosition(
    val progress: Double,
    val positionMs: Long?,
    val durationMs: Long?,
)

/** One queue entry the [fm.bae.app.ui.playback.QueueScreen] renders. The UI projection
 *  of the player's internal queue metadata: [durationClock] is the track length
 *  as a clock label's fields (null when core reports none), which the row renders
 *  directly; [coverImage] is the cover the row fetches bytes for. [entryId] is the
 *  per-instance id the row keys on and that remove/reorder/skip target — unique
 *  even when the same track is queued twice. */
data class QueueItem(
    val entryId: String,
    val trackId: String,
    val title: String,
    val artist: String,
    val albumTitle: String,
    val durationClock: BridgeDurationClock?,
    val coverImage: BridgeImageRef?,
)

/** The context lane (the release being played from): the first page of its
 *  not-yet-played tail, the tail's full length, further pages fetched via
 *  [fm.bae.app.playback.BaeCorePlayer.loadUpcomingRange], plus whether it was
 *  ordered by shuffle (the UI shows a shuffle indicator when so). Rendered as a
 *  section distinct from the manual lane.
 *
 *  [upcoming] is only the initial window core resolved eagerly — the tail is
 *  library-scaled. [pagedUpcoming] holds indices fetched past that window,
 *  keyed by absolute index; [itemAt] reads either uniformly. */
data class QueueContext(
    val kind: BridgePlaybackSourceKind,
    val shuffled: Boolean,
    val upcoming: List<QueueItem>,
    val upcomingTotal: Int,
    val pagedUpcoming: Map<Int, QueueItem> = emptyMap(),
) {
    fun itemAt(index: Int): QueueItem? = upcoming.getOrNull(index) ?: pagedUpcoming[index]
}

/** The queue's two lanes the [fm.bae.app.ui.playback.QueueScreen] renders as distinct
 *  sections: the manual lane ("Up Next") and the [context] (or null when nothing
 *  plays from a release). Kept separate, not flattened. [revision] is the queue
 *  revision this projection was built from — a UI stamps its
 *  [BaeCorePlayer.loadUpcomingRange] fetches against it and drops a reply
 *  computed under a since-superseded revision. */
data class QueueProjection(
    val manual: List<QueueItem>,
    val context: QueueContext?,
    val revision: ULong = 0u,
) {
    companion object {
        val EMPTY = QueueProjection(manual = emptyList(), context = null)
    }
}

/**
 * The playback-state intake [BaeCorePlayer] exposes to [fm.bae.app.data.UiEventAdapter].
 * Event routing depends on this contract, not the concrete player, so playback
 * handling can be exercised against a fake sink.
 */
interface PlaybackEventSink {
    fun onLoading(
        trackId: String,
        track: BridgeLoadingTrackInfo?,
    )

    fun onPlaying(event: BridgeUiEvent.PlaybackPlaying)

    fun onPaused(event: BridgeUiEvent.PlaybackPaused)

    fun onStopped()

    fun onProgress(
        trackId: String,
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    )

    fun onSeeked(
        trackId: String,
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    )

    fun onRepeatModeChanged(mode: BridgeRepeatMode)

    fun onQueueUpdated(
        manual: List<BridgeQueueEntry>,
        context: BridgePlaybackContext?,
        hasNext: Boolean,
        hasPrevious: Boolean,
        revision: ULong,
    )

    fun onVolumeChanged(volume: Float)

    fun onMuteChanged(isMuted: Boolean)

    fun onQueueItemsAdded(count: Int)
}
