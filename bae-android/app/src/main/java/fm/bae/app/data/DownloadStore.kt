package fm.bae.app.data

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeDownloadTransferChange
import uniffi.bae_bridge.BridgeDownloadTransferProgress

/**
 * Mirror of core's download queue's counts: how many entries are in each phase
 * and whether it is paused, which core sends each time they change, or the
 * failure of the last read in their place; and each downloading release's
 * latest bytes, which the subscription writes on the thread it arrives on and
 * the View drawing that release reads once per display frame. The entries
 * themselves are the Downloads screen's windows ([DownloadPageStore]).
 */
class DownloadStore(
    initialCounts: BridgeDownloadCounts,
) {
    private val _state = MutableStateFlow<LiveValue<BridgeDownloadCounts>>(LiveValue.Current(initialCounts))
    val state: StateFlow<LiveValue<BridgeDownloadCounts>> = _state.asStateFlow()

    /** Each release's download progress; null while it is not downloading. Read and written on the main thread. */
    private val transfers = mutableMapOf<String, MutableStateFlow<BridgeDownloadTransferProgress?>>()

    /** Each downloading release's latest progress, written on any thread. */
    private val latest = LatestByKey<BridgeDownloadTransferProgress>()

    /** [releaseId]'s latest download progress, for the View that reads it each display frame. */
    fun latestProgress(releaseId: String) = LatestSource(latest, releaseId)

    /** How far [releaseId]'s download has gone; null while none has arrived for it. */
    fun progress(releaseId: String): StateFlow<BridgeDownloadTransferProgress?> = transfer(releaseId).asStateFlow()

    private fun transfer(releaseId: String): MutableStateFlow<BridgeDownloadTransferProgress?> =
        transfers.getOrPut(releaseId) { MutableStateFlow(null) }

    /** Apply one change the transfer subscription delivered to the latest values, on any thread. */
    fun applyLatestTransferChange(change: BridgeDownloadTransferChange) {
        when (change) {
            is BridgeDownloadTransferChange.Updated -> {
                latest.set(change.releaseId, change.progress)
            }

            is BridgeDownloadTransferChange.Removed -> {
                latest.remove(change.releaseId)
            }

            is BridgeDownloadTransferChange.Reset -> {
                latest.reset(change.transfers.associate { it.releaseId to it.progress })
            }
        }
    }

    /** Apply one change the transfer subscription delivered. */
    fun applyTransferChange(change: BridgeDownloadTransferChange) {
        when (change) {
            is BridgeDownloadTransferChange.Updated -> {
                transfer(change.releaseId).value = change.progress
            }

            is BridgeDownloadTransferChange.Removed -> {
                transfer(change.releaseId).value = null
            }

            is BridgeDownloadTransferChange.Reset -> {
                val listed = change.transfers.associate { it.releaseId to it.progress }
                transfers.forEach { (releaseId, flow) -> flow.value = listed[releaseId] }
                listed.forEach { (releaseId, progress) -> transfer(releaseId).value = progress }
            }
        }
    }

    fun setCounts(counts: BridgeDownloadCounts) {
        _state.value = LiveValue.Current(counts)
    }

    fun fail(error: DisplayError) {
        _state.value = LiveValue.Failed(error)
    }
}
