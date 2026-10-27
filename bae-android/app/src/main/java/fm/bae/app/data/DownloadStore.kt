package fm.bae.app.data

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeDownloadSnapshot
import uniffi.bae_bridge.BridgeDownloadTransferChange
import uniffi.bae_bridge.BridgeDownloadTransferProgress

/**
 * Mirror of core's download queue: its entries, phases and pause,
 * which core sends each time they change, or the failure of the last read in
 * their place; and each downloading release's bytes, kept in that release's
 * own flow so bytes arriving recompose only what collects that release.
 */
class DownloadStore(
    initialSnapshot: BridgeDownloadSnapshot,
) {
    private val _state = MutableStateFlow<LiveValue<BridgeDownloadSnapshot>>(LiveValue.Current(initialSnapshot))
    val state: StateFlow<LiveValue<BridgeDownloadSnapshot>> = _state.asStateFlow()

    /** Each release's download progress; null while it is not downloading. Read and written on the main thread. */
    private val transfers = mutableMapOf<String, MutableStateFlow<BridgeDownloadTransferProgress?>>()

    /** How far [releaseId]'s download has gone; null while none has arrived for it. */
    fun progress(releaseId: String): StateFlow<BridgeDownloadTransferProgress?> = transfer(releaseId).asStateFlow()

    private fun transfer(releaseId: String): MutableStateFlow<BridgeDownloadTransferProgress?> =
        transfers.getOrPut(releaseId) { MutableStateFlow(null) }

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

    fun setSnapshot(snapshot: BridgeDownloadSnapshot) {
        _state.value = LiveValue.Current(snapshot)
    }

    fun fail(error: DisplayError) {
        _state.value = LiveValue.Failed(error)
    }
}
