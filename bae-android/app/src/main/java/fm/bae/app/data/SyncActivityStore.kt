package fm.bae.app.data

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeSyncActivity

/**
 * Everything sync is doing on this device, as core reports it: the top bar's
 * Activity button and the sheet it opens read it here. The sync-activity value
 * stream is its only writer.
 */
class SyncActivityStore {
    private val mutableActivity = MutableStateFlow<BridgeSyncActivity?>(null)
    val activity: StateFlow<BridgeSyncActivity?> = mutableActivity.asStateFlow()

    /** Why the activity couldn't be read, until it is read again. */
    private val mutableError = MutableStateFlow<DisplayError?>(null)
    val error: StateFlow<DisplayError?> = mutableError.asStateFlow()

    fun apply(value: BridgeSyncActivity) {
        mutableActivity.value = value
        mutableError.value = null
    }

    fun fail(error: DisplayError) {
        mutableError.value = error
    }
}
