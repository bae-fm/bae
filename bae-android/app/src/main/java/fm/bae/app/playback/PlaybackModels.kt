package fm.bae.app.playback

import uniffi.bae_bridge.BridgeImageRef
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

/** What the queue plays from (a release, a list of tracks or the library), and the not-yet-played
 *  tracks after the current one: [upcomingTotal] of them, of which [upcoming] holds those read
 *  through [BaeCorePlayer.loadUpcomingRange], keyed by index in the whole tail. */
data class QueueContext(
    val kind: BridgePlaybackSourceKind,
    val upcomingTotal: Int,
    val upcoming: Map<Int, BridgeQueueEntry> = emptyMap(),
) {
    fun itemAt(index: Int): BridgeQueueEntry? = upcoming[index]
}

/** The queue as two sections: tracks added by hand ([manual]) and the [context] played from, or
 *  null when there is none. [shuffled] is core's shuffle mode, which holds with no context too.
 *  [manualVersion] and [contextVersion] are core's versions of each: how
 *  many writes have changed that lane, as its read last carried them. */
data class QueueProjection(
    val manual: List<BridgeQueueEntry>,
    val context: QueueContext?,
    val shuffled: Boolean = false,
    val manualVersion: ULong = 0u,
    val contextVersion: ULong = 0u,
) {
    companion object {
        val EMPTY = QueueProjection(manual = emptyList(), context = null)
    }
}
