package fm.bae.app.data

import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncStatusSnapshot

/**
 * The last sync failure as a person sees it, whether Reconnect is offered beside it (core says a start failed and left
 * sync not running with storage set up on this device), and whether sync found another copy of this device, which only
 * a reset answers.
 */
data class SyncFailure(
    val error: DisplayError,
    val offersReconnect: Boolean,
    val offersDeviceReset: Boolean,
)

/**
 * What the sync status line shows: checking, until the sync status's first read; unreadable, after a read that failed,
 * until the next succeeds; no storage, while this device has none set up; or the indicator of the status as read.
 */
sealed interface SyncLine {
    data object Checking : SyncLine

    data object Unreadable : SyncLine

    /**
     * This device has no storage set up, so there is no sync to describe. Under a recorded location it means this
     * device no longer holds its credentials for it, and Settings offers signing in to it again in place of the line.
     */
    data object NoStorage : SyncLine

    data class Read(
        val indicator: BridgeSyncIndicator,
    ) : SyncLine
}

/** Runtime sync state. The sync-status value stream is its only writer. */
class SyncStatusStore {
    private val _error = MutableStateFlow<SyncFailure?>(null)
    val error: StateFlow<SyncFailure?> = _error.asStateFlow()

    private val _line = MutableStateFlow<SyncLine>(SyncLine.Checking)
    val line: StateFlow<SyncLine> = _line.asStateFlow()

    /** What coven is waiting on, each with the action the person can take on it; none until the status is read. */
    private val _pending = MutableStateFlow<List<BridgePendingRecord>?>(null)
    val pending: StateFlow<List<BridgePendingRecord>?> = _pending.asStateFlow()

    fun apply(
        status: BridgeSyncStatusSnapshot,
        errors: ErrorDisplays,
    ) {
        _error.value =
            status.error?.let { error ->
                errors.display(error)?.let { shown ->
                    SyncFailure(
                        shown,
                        offersReconnect = status.reconnectOffered,
                        offersDeviceReset =
                            (error as? BridgeException.Diagnostic)?.category is BridgeErrorCategory.DeviceResetRequired,
                    )
                }
            }
        _line.value = status.indicator?.let(SyncLine::Read) ?: SyncLine.NoStorage
        _pending.value = status.pending
    }

    /** The sync status couldn't be read: the line shows the read's failure until a read succeeds. */
    fun fail(error: DisplayError) {
        _error.value = SyncFailure(error, offersReconnect = false, offersDeviceReset = false)
        _line.value = SyncLine.Unreadable
    }
}
