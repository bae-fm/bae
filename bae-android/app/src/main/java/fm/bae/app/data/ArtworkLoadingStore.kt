package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeEagerCacheFillStatus

class ArtworkLoadingStore(
    private val cancelAction: () -> Unit,
) {
    private val mutableStatus =
        MutableStateFlow<BridgeEagerCacheFillStatus>(
            BridgeEagerCacheFillStatus.NotRunning,
        )
    val status: StateFlow<BridgeEagerCacheFillStatus> = mutableStatus.asStateFlow()

    fun apply(status: BridgeEagerCacheFillStatus) {
        mutableStatus.value = status
    }

    fun cancel() = cancelAction()
}
