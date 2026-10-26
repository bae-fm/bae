package fm.bae.app.data

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeOutboxPauseState
import uniffi.bae_bridge.BridgeOutboxSnapshot

/**
 * Mirror of core's cloud-upload queue. Core sends the queue each time it
 * changes — including each pause/resume — so the Settings pause toggle renders
 * its state from [BridgeOutboxSnapshot.pauseState] rather than tracking it
 * optimistically, or the failure of the last read in its place.
 */
class OutboxStore(
    initialSnapshot: BridgeOutboxSnapshot,
) {
    private val _state = MutableStateFlow<LiveValue<BridgeOutboxSnapshot>>(LiveValue.Current(initialSnapshot))
    val state: StateFlow<LiveValue<BridgeOutboxSnapshot>> = _state.asStateFlow()

    fun setSnapshot(snapshot: BridgeOutboxSnapshot) {
        _state.value = LiveValue.Current(snapshot)
    }

    fun fail(error: DisplayError) {
        _state.value = LiveValue.Failed(error)
    }
}

internal val BridgeOutboxSnapshot.pauseRequested: Boolean
    get() = pauseState != BridgeOutboxPauseState.RUNNING
