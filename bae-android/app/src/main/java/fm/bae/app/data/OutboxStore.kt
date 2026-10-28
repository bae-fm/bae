package fm.bae.app.data

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeUploadTransfer

/**
 * Whether the person paused uploads, as core's live read says each time it
 * changes, so the Settings pause toggle renders it rather than tracking it
 * optimistically, or the failure of the last read in its place. The files
 * being sent together arrive apart, many times a second: the subscription
 * writes the latest on the thread it arrives on, and the View drawing it
 * reads it once per display frame while files are being sent.
 */
class OutboxStore(
    initialUploadsPaused: Boolean,
) {
    private val _uploadsPaused = MutableStateFlow<LiveValue<Boolean>>(LiveValue.Current(initialUploadsPaused))
    val uploadsPaused: StateFlow<LiveValue<Boolean>> = _uploadsPaused.asStateFlow()

    private val filesBeingSent = LatestOne<BridgeUploadTransfer>()

    /** The latest of the files being sent together, while any is, for the View that reads it each frame. */
    val totalProgress: LatestSource<BridgeUploadTransfer> get() = filesBeingSent.source

    fun setUploadsPaused(paused: Boolean) {
        _uploadsPaused.value = LiveValue.Current(paused)
    }

    fun fail(error: DisplayError) {
        _uploadsPaused.value = LiveValue.Failed(error)
    }

    /** Hold the files being sent together, on any thread: while none is being sent they are idle. */
    fun setTotalTransfer(transfer: BridgeUploadTransfer) {
        if (transfer.bar == null) filesBeingSent.clear() else filesBeingSent.set(transfer)
    }

    /** Draw no transfer once its read failed: the bytes it held are no longer known to be current. */
    fun forgetTotalTransfer() {
        filesBeingSent.clear()
    }
}
