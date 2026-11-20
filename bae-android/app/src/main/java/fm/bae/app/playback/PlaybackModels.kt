package fm.bae.app.playback

import android.content.Context
import fm.bae.app.coreString
import fm.bae.app.creditLineText
import fm.bae.app.text
import uniffi.bae_bridge.BridgeNowPlayingDisplay
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeSidePausePrompt
import uniffi.bae_bridge.BridgeTrackDisplay
import uniffi.bae_bridge.bridgeSidePauseKeepPausingKey
import uniffi.bae_bridge.bridgeSidePauseTitleKey

/** The track the in-app player shows: its id, what the library shows of it or that the library no longer holds it,
 *  and the prompt of a pause between sides. */
data class NowPlaying(
    val trackId: String,
    val display: BridgeNowPlayingDisplay,
    val sidePausePrompt: SidePausePrompt?,
)

/** What the library shows of the track, or null when the library no longer holds it. */
val BridgeNowPlayingDisplay.library: BridgeTrackDisplay?
    get() = (this as? BridgeNowPlayingDisplay.Library)?.display

/** The track's title as the player shows it: the library's title, or, when the library no longer holds the track,
 *  the title core's catalog gives a removed track. */
fun BridgeNowPlayingDisplay.title(context: Context): String {
    val shown = library ?: return context.coreString("core.playback.removed_track")
    return shown.title
}

/** The track's artists in the person's language, or null when the library no longer holds the track. */
fun BridgeNowPlayingDisplay.artist(context: Context): String? = library?.let { context.creditLineText(it.artistLine) }

/**
 * A side-pause prompt with the catalog keys its title and checkbox are worded
 * from, looked up from the side or disc that ended when the player receives
 * it.
 */
data class SidePausePrompt(
    val prompt: BridgeSidePausePrompt,
    val titleKey: String,
    val keepPausingKey: String,
) {
    /** The prompt's title, "Side A ended", from the heading of the side or disc that ended. */
    fun title(context: Context): String = context.coreString(titleKey, mapOf("heading" to prompt.ended.text(context)))

    companion object {
        fun of(prompt: BridgeSidePausePrompt): SidePausePrompt =
            SidePausePrompt(
                prompt = prompt,
                titleKey = bridgeSidePauseTitleKey(prompt.ended),
                keepPausingKey = bridgeSidePauseKeepPausingKey(prompt.ended),
            )
    }
}

/** What the queue plays from (a release, a list of tracks or the library), and the not-yet-played
 *  tracks after the current one: [upcomingTotal] of them, of which [upcoming] holds those read
 *  through [BaeCorePlayer.showUpcoming], keyed by index in the whole tail. */
data class QueueContext(
    val kind: BridgePlaybackSourceKind,
    val upcomingTotal: Int,
    val upcoming: Map<Int, BridgeQueueEntry> = emptyMap(),
) {
    fun itemAt(index: Int): BridgeQueueEntry? = upcoming[index]
}

/** The queue as two sections: tracks added by hand ([manual]) and the [context] played from, or
 *  null when there is none. [shuffled] is core's shuffle mode, which holds with no context too.
 *  [upNextCount] and [isEmpty] are core's count of [manual] and whether nothing is to play after the
 *  playing track. */
data class QueueProjection(
    val manual: List<BridgeQueueEntry>,
    val context: QueueContext?,
    val shuffled: Boolean = false,
    val upNextCount: UInt = 0u,
    val isEmpty: Boolean = true,
) {
    companion object {
        val EMPTY = QueueProjection(manual = emptyList(), context = null)
    }
}
