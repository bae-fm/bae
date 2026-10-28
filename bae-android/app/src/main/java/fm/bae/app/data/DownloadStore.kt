package fm.bae.app.data

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeDownloadTransferProgress

/**
 * Mirror of core's download queue's counts: how many entries are in each phase
 * and whether it is paused, which core sends each time they change, or the
 * failure of the last read in their place; and the stream of each downloading
 * release's latest bytes, which the bridge holds and the View drawing that
 * release reads once per display frame. The entries themselves are the
 * Downloads screen's windows ([DownloadPageStore]).
 */
class DownloadStore(
    initialCounts: BridgeDownloadCounts,
    /** Each downloading release's latest progress: the session's to feed, or a preview's fixed values. */
    val releaseProgress: LatestStream<BridgeDownloadTransferProgress> = LatestStream.downloads(),
) {
    private val _state = MutableStateFlow<LiveValue<BridgeDownloadCounts>>(LiveValue.Current(initialCounts))
    val state: StateFlow<LiveValue<BridgeDownloadCounts>> = _state.asStateFlow()

    /** [releaseId]'s latest download progress, for the View that reads it each display frame. */
    fun latestProgress(releaseId: String) = releaseProgress.source(releaseId)

    fun setCounts(counts: BridgeDownloadCounts) {
        _state.value = LiveValue.Current(counts)
    }

    fun fail(error: DisplayError) {
        _state.value = LiveValue.Failed(error)
    }
}
