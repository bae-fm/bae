package fm.bae.app.data

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeDownloadSnapshot

/**
 * Mirror of core's in-memory download queue snapshot. Core emits the full
 * snapshot on every queue change; Compose renders the bridge payload directly,
 * or the failure of the last read in its place.
 */
class DownloadStore(
    initialSnapshot: BridgeDownloadSnapshot,
) {
    private val _state = MutableStateFlow<LiveValue<BridgeDownloadSnapshot>>(LiveValue.Current(initialSnapshot))
    val state: StateFlow<LiveValue<BridgeDownloadSnapshot>> = _state.asStateFlow()

    fun setSnapshot(snapshot: BridgeDownloadSnapshot) {
        _state.value = LiveValue.Current(snapshot)
    }

    fun fail(error: DisplayError) {
        _state.value = LiveValue.Failed(error)
    }
}
