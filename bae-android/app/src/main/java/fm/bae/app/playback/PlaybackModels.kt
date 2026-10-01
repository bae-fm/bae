package fm.bae.app.playback

import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeSidePausePrompt
import uniffi.bae_bridge.bridgePauseBoundaryCountdownKey
import uniffi.bae_bridge.bridgePauseBoundaryKeepPausingKey
import uniffi.bae_bridge.bridgePauseBoundaryTitleKey

data class NowPlaying(
    val trackId: String,
    val title: String,
    val artist: String,
    /** The cover the bar fetches bytes for, or null when there is none. */
    val coverImage: BridgeImageRef?,
    val sidePausePrompt: SidePausePrompt?,
)

/**
 * A side-pause prompt with the catalog keys its title, countdown and checkbox
 * are worded from, looked up from the kind of boundary that ended when the
 * player receives it, so the dialog renders without calling the bridge.
 */
data class SidePausePrompt(
    val prompt: BridgeSidePausePrompt,
    val titleKey: String,
    val countdownKey: String,
    val keepPausingKey: String,
) {
    companion object {
        fun of(prompt: BridgeSidePausePrompt): SidePausePrompt =
            SidePausePrompt(
                prompt = prompt,
                titleKey = bridgePauseBoundaryTitleKey(prompt.boundary),
                countdownKey = bridgePauseBoundaryCountdownKey(prompt.boundary),
                keepPausingKey = bridgePauseBoundaryKeepPausingKey(prompt.boundary),
            )
    }
}

/**
 * The seek bar's position: [progress] in [0,1] for the slider, and milliseconds
 * the bar turns into clock labels through the bridge. [positionMs] is null when
 * nothing is playing; [durationMs] is null when the length is unknown.
 */
data class PlaybackPosition(
    val progress: Double,
    val positionMs: Long?,
    val durationMs: Long?,
)

/** The release or library the queue plays from, and the not-yet-played tracks
 *  after the current one. [upcoming] is only the first page of [upcomingTotal];
 *  [pagedUpcoming] holds later tracks read through
 *  [BaeCorePlayer.loadUpcomingRange], keyed by index in the whole tail. */
data class QueueContext(
    val kind: BridgePlaybackSourceKind,
    val shuffled: Boolean,
    val upcoming: List<BridgeQueueEntry>,
    val upcomingTotal: Int,
    val pagedUpcoming: Map<Int, BridgeQueueEntry> = emptyMap(),
) {
    fun itemAt(index: Int): BridgeQueueEntry? = upcoming.getOrNull(index) ?: pagedUpcoming[index]
}

/** The queue as two sections: tracks added by hand ([manual]) and the
 *  [context] played from, or null when there is none. [revision] is the queue
 *  revision it was built from. */
data class QueueProjection(
    val manual: List<BridgeQueueEntry>,
    val context: QueueContext?,
    val revision: ULong = 0u,
) {
    companion object {
        val EMPTY = QueueProjection(manual = emptyList(), context = null)
    }
}

/** Playback events [fm.bae.app.data.UiEventAdapter] passes to [BaeCorePlayer]. */
interface PlaybackEventSink {
    fun onQueueItemsAdded(count: Int)
}
