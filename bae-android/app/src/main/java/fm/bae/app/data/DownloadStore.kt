package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeDownloadSnapshot

/**
 * Mirror of core's in-memory download queue snapshot. Core emits the full
 * snapshot on every queue change; Compose renders the bridge payload directly.
 */
class DownloadStore(
    initialSnapshot: BridgeDownloadSnapshot,
) {
    private val _snapshot = MutableStateFlow(initialSnapshot)
    val snapshot: StateFlow<BridgeDownloadSnapshot> = _snapshot.asStateFlow()

    fun setSnapshot(snapshot: BridgeDownloadSnapshot) {
        _snapshot.value = snapshot
    }
}
