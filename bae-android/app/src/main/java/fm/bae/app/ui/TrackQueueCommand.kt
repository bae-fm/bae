package fm.bae.app.ui

import fm.bae.app.OpenLibrary

/**
 * Adds the track [trackId] to Up Next with [command], which [operation] names in the log. The snackbar confirms how
 * many tracks the command added.
 */
internal fun OpenLibrary.runTrackQueueCommand(
    trackId: String,
    operation: String,
    command: suspend (List<String>) -> UInt,
) {
    messages.perform("$operation $trackId") {
        playback.onTracksAdded(command(listOf(trackId)).toInt())
    }
}
