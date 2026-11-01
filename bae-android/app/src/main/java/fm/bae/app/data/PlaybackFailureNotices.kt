package fm.bae.app.data

import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgePlaybackValues

/**
 * Which playback failures are still to be shown. A playback value carries
 * why playback last failed until it next plays, numbered, so a value that
 * reads the same failure again shows nothing.
 */
class PlaybackFailureNotices {
    private var shownNumber: ULong? = null

    /** Why playback failed, the first time a value carries that failure. */
    fun toShow(values: BridgePlaybackValues): BridgePlaybackErrorReason? {
        val failure = values.failure?.takeIf { it.number != shownNumber } ?: return null
        shownNumber = failure.number
        return failure.reason
    }
}
