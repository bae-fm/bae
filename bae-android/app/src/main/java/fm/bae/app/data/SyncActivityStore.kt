package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeArrivals
import uniffi.bae_bridge.BridgeReleaseArrival
import uniffi.bae_bridge.BridgeSyncActivity

/**
 * Everything sync is doing on this device, as core reports it: the top bar's
 * indicator, the list it opens, and the quiet lines on albums whose files are
 * still arriving from another device all read it here. The sync-activity value
 * stream is its only writer.
 */
class SyncActivityStore(
    private val cancelArtworkAction: () -> Unit,
    private val retryNowAction: () -> Unit,
) {
    private val mutableActivity = MutableStateFlow<BridgeSyncActivity?>(null)
    val activity: StateFlow<BridgeSyncActivity?> = mutableActivity.asStateFlow()

    /** The last failure to read the activity, as its line. */
    private val mutableError = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = mutableError.asStateFlow()

    fun apply(value: BridgeSyncActivity) {
        mutableActivity.value = value
        mutableError.value = null
    }

    fun fail(line: String) {
        mutableError.value = line
    }

    /** Stop the artwork downloads. */
    fun cancelArtwork() = cancelArtworkAction()

    /** The person's retry: every ordinary wait ends and sync runs now. */
    fun retryNow() = retryNowAction()
}

/** The indicator shows while bytes move or something needs the person, and is absent otherwise. */
fun syncActivityVisible(
    activity: BridgeSyncActivity?,
    error: String?,
): Boolean = error != null || (activity != null && (activity.moving || activity.needsAttention))

/** The device an album is still arriving from, if any of its files is. */
fun BridgeSyncActivity?.arrivingFrom(albumId: String): String? {
    val ready = this?.arrivals as? BridgeArrivals.Ready
    return ready?.albums?.get(albumId)
}

/** What of a release is still arriving, if anything is. */
fun BridgeSyncActivity?.arrival(releaseId: String): BridgeReleaseArrival? {
    val ready = this?.arrivals as? BridgeArrivals.Ready
    return ready?.releases?.get(releaseId)
}
