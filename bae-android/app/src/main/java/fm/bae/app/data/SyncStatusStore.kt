package fm.bae.app.data

import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgeSyncShown
import uniffi.bae_bridge.BridgeSyncStatusSnapshot

/**
 * A storage setup that ended undone with no setup screen there to hear it, such as one bae was stopped partway
 * through: why, and the [operation] that dismisses it.
 */
data class UndoneSetup(
    val operation: Long,
    val error: DisplayError,
)

/**
 * What the Sync page's top block shows: nothing until the sync status's first read; unreadable, after a read that
 * failed, until the next succeeds; or the state as read.
 */
sealed interface SyncLine {
    data object Checking : SyncLine

    data object Unreadable : SyncLine

    data class Read(
        val shown: BridgeSyncShown,
    ) : SyncLine
}

/** Runtime sync state. The sync-status value stream is its only writer. */
class SyncStatusStore {
    private val _line = MutableStateFlow<SyncLine>(SyncLine.Checking)
    val line: StateFlow<SyncLine> = _line.asStateFlow()

    /** What coven is waiting on, each with the action the person can take on it; none until the status is read. */
    private val _pending = MutableStateFlow<List<BridgePendingRecord>?>(null)
    val pending: StateFlow<List<BridgePendingRecord>?> = _pending.asStateFlow()

    /** Whether the last read held a record a phone can't have: a bug, reported, and kept out of [pending]. */
    private val _recordsDropped = MutableStateFlow(false)
    val recordsDropped: StateFlow<Boolean> = _recordsDropped.asStateFlow()
    private val phoneRecords = PhonePendingRecords()

    /**
     * A storage setup runs: a setup screen's, or one past its point of no return that sync's connection finishes after
     * a relaunch. It records the library's location as it finishes, so Settings reads the location again once this
     * turns false.
     */
    private val _setupRunning = MutableStateFlow(false)
    val setupRunning: StateFlow<Boolean> = _setupRunning.asStateFlow()

    private val _undoneSetup = MutableStateFlow<UndoneSetup?>(null)
    val undoneSetup: StateFlow<UndoneSetup?> = _undoneSetup.asStateFlow()

    fun apply(
        status: BridgeSyncStatusSnapshot,
        errors: ErrorDisplays,
        bugs: BugReports,
    ) {
        _line.value = SyncLine.Read(status.shown)
        val taken = phoneRecords.take(status.pending, "list the pending records", bugs::reportShownInPlace)
        _pending.value = taken.listed
        _recordsDropped.value = taken.onlyAMacHas.isNotEmpty()
        _setupRunning.value = status.setupRunning
        _undoneSetup.value =
            status.undoneSetup?.let { undone ->
                errors.display(undone.error)?.let { shown -> UndoneSetup(undone.operation, shown) }
            }
    }

    /** The sync status couldn't be read: the page says so until a read succeeds. */
    fun fail() {
        _line.value = SyncLine.Unreadable
    }
}
