package fm.bae.app.data

import fm.bae.app.ErrorLines
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePendingRecords
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncStatusSnapshot

/**
 * The last sync failure as a line, whether reconnecting is what fixes it, and
 * whether sync found another copy of this device, which only a reset answers.
 */
data class SyncFailure(
    val message: String,
    val canReconnect: Boolean,
    val offersDeviceReset: Boolean,
)

/** Runtime sync state. The sync-status value stream is its only writer. */
class SyncStatusStore {
    private val _snapshot = MutableStateFlow<BridgeSyncStatusSnapshot?>(null)
    val snapshot: StateFlow<BridgeSyncStatusSnapshot?> = _snapshot.asStateFlow()

    private val _error = MutableStateFlow<SyncFailure?>(null)
    val error: StateFlow<SyncFailure?> = _error.asStateFlow()

    private val _indicator = MutableStateFlow<BridgeSyncIndicator>(BridgeSyncIndicator.Loading)
    val indicator: StateFlow<BridgeSyncIndicator> = _indicator.asStateFlow()

    /** What coven is waiting on, each with the action the person can take on it. */
    private val _pending = MutableStateFlow<BridgePendingRecords>(BridgePendingRecords.Loading)
    val pending: StateFlow<BridgePendingRecords> = _pending.asStateFlow()

    fun apply(
        status: BridgeSyncStatusSnapshot,
        errors: ErrorLines,
    ) {
        _snapshot.value = status
        // Disconnected, reconnecting is the remedy; connected, the failure names its own.
        _error.value =
            status.error?.let { error ->
                errors.line(error)?.let { line ->
                    SyncFailure(
                        line,
                        canReconnect = !status.connected,
                        offersDeviceReset =
                            (error as? BridgeException.Diagnostic)?.category is BridgeErrorCategory.DeviceResetRequired,
                    )
                }
            }
        _indicator.value = status.indicator
        _pending.value = status.pending
    }
}
