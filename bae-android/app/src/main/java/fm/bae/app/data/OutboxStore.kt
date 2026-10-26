package fm.bae.app.data

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeOutboxPauseState
import uniffi.bae_bridge.BridgeOutboxSnapshot
import uniffi.bae_bridge.BridgeUploadTransfer

/**
 * Mirror of core's cloud-upload queue summary. Core sends it each time the
 * queue changes — including each pause/resume — so the Settings pause toggle
 * renders its state from [BridgeOutboxSnapshot.pauseState] rather than
 * tracking it optimistically, or the failure of the last read in its place.
 * The whole queue's transfer arrives apart, with each part a provider
 * confirms, so only what draws it follows it.
 */
class OutboxStore(
    initialSnapshot: BridgeOutboxSnapshot,
) {
    private val _state = MutableStateFlow<LiveValue<BridgeOutboxSnapshot>>(LiveValue.Current(initialSnapshot))
    val state: StateFlow<LiveValue<BridgeOutboxSnapshot>> = _state.asStateFlow()

    private val _totalTransfer = MutableStateFlow<BridgeUploadTransfer?>(null)

    /** How far the files being sent have gone together, and how fast; null until the transfers first arrive. */
    val totalTransfer: StateFlow<BridgeUploadTransfer?> = _totalTransfer.asStateFlow()

    fun setSnapshot(snapshot: BridgeOutboxSnapshot) {
        _state.value = LiveValue.Current(snapshot)
    }

    fun fail(error: DisplayError) {
        _state.value = LiveValue.Failed(error)
    }

    /** Hold the whole queue's transfer the total subscription delivered. */
    fun setTotalTransfer(transfer: BridgeUploadTransfer) {
        _totalTransfer.value = transfer
    }
}

internal val BridgeOutboxSnapshot.pauseRequested: Boolean
    get() = pauseState != BridgeOutboxPauseState.RUNNING
