package fm.bae.app.data

import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
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

    private val _undoneSetup = MutableStateFlow<UndoneSetup?>(null)
    val undoneSetup: StateFlow<UndoneSetup?> = _undoneSetup.asStateFlow()

    fun apply(
        status: BridgeSyncStatusSnapshot,
        errors: ErrorDisplays,
    ) {
        _line.value = SyncLine.Read(status.shown)
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
