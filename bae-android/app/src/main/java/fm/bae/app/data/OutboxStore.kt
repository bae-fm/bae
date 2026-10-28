package fm.bae.app.data

import fm.bae.app.DisplayError
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeUploadTransfer

/**
 * Whether the person paused uploads, as core's live read says each time it
 * changes, so the Settings pause toggle renders it rather than tracking it
 * optimistically, or the failure of the last read in its place; and the
 * stream of the files being sent together, which the bridge holds and the
 * View drawing it reads once per display frame while files are being sent.
 */
class OutboxStore(
    initialUploadsPaused: Boolean,
    /** The files being sent together while any is: the session's to feed, or a preview's fixed value. */
    val filesBeingSent: LatestStream<BridgeUploadTransfer> = LatestStream.uploadTotal(),
) {
    private val _uploadsPaused = MutableStateFlow<LiveValue<Boolean>>(LiveValue.Current(initialUploadsPaused))
    val uploadsPaused: StateFlow<LiveValue<Boolean>> = _uploadsPaused.asStateFlow()

    /** The latest of the files being sent together, while any is, for the View that reads it each frame. */
    val totalProgress: LatestSource<BridgeUploadTransfer> get() = filesBeingSent.only

    fun setUploadsPaused(paused: Boolean) {
        _uploadsPaused.value = LiveValue.Current(paused)
    }

    fun fail(error: DisplayError) {
        _uploadsPaused.value = LiveValue.Failed(error)
    }
}
