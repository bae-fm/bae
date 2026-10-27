package fm.bae.app.data

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeUploadTransfer

/**
 * Whether the person paused uploads, as core's live read says each time it
 * changes, so the Settings pause toggle renders it rather than tracking it
 * optimistically, or the failure of the last read in its place. The whole
 * queue's transfer arrives apart, with each part a provider confirms, so only
 * what draws it follows it.
 */
class OutboxStore(
    initialUploadsPaused: Boolean,
) {
    private val _uploadsPaused = MutableStateFlow<LiveValue<Boolean>>(LiveValue.Current(initialUploadsPaused))
    val uploadsPaused: StateFlow<LiveValue<Boolean>> = _uploadsPaused.asStateFlow()

    private val _totalTransfer = MutableStateFlow<BridgeUploadTransfer?>(null)

    /** How far the files being sent have gone together, and how fast; null until the transfers first arrive. */
    val totalTransfer: StateFlow<BridgeUploadTransfer?> = _totalTransfer.asStateFlow()

    fun setUploadsPaused(paused: Boolean) {
        _uploadsPaused.value = LiveValue.Current(paused)
    }

    fun fail(error: DisplayError) {
        _uploadsPaused.value = LiveValue.Failed(error)
    }

    /** Hold the whole queue's transfer the total subscription delivered. */
    fun setTotalTransfer(transfer: BridgeUploadTransfer) {
        _totalTransfer.value = transfer
    }
}
