package fm.bae.app.data

import fm.bae.app.ErrorLines
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeBlockedSyncOperation
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.bridgeSyncIndicator

data class SyncFailure(
    val message: String,
    val canReconnect: Boolean,
)

/** Runtime sync state. The sync-status value stream is its only writer. */
class SyncStatusStore(
    private val indicatorFor: (BridgeSyncStatusSnapshot) -> BridgeSyncIndicator = ::bridgeSyncIndicator,
) {
    private val _snapshot = MutableStateFlow<BridgeSyncStatusSnapshot?>(null)
    val snapshot: StateFlow<BridgeSyncStatusSnapshot?> = _snapshot.asStateFlow()

    private val _error = MutableStateFlow<SyncFailure?>(null)
    val error: StateFlow<SyncFailure?> = _error.asStateFlow()

    private val _indicator = MutableStateFlow<BridgeSyncIndicator>(BridgeSyncIndicator.Idle)
    val indicator: StateFlow<BridgeSyncIndicator> = _indicator.asStateFlow()

    /**
     * The durable sync operations the last completed cycle left waiting on a
     * person. Each is retried by handing its `id` back to the bridge; the list
     * empties when a cycle reports nothing waiting.
     */
    private val _blocked = MutableStateFlow<List<BridgeBlockedSyncOperation>>(emptyList())
    val blocked: StateFlow<List<BridgeBlockedSyncOperation>> = _blocked.asStateFlow()

    fun apply(
        status: BridgeSyncStatusSnapshot,
        errors: ErrorLines,
    ) {
        _snapshot.value = status
        _error.value = status.error?.let(errors::line)?.let { SyncFailure(it, status.canReconnect) }
        _indicator.value = indicatorFor(status)
        _blocked.value = status.blocked
    }
}
