package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import uniffi.bae_bridge.BridgeEagerCacheFillStatus

data class ArtworkLoadingState(
    val status: BridgeEagerCacheFillStatus,
    val dismissed: Boolean = false,
)

class ArtworkLoadingStore(
    private val cancelAction: () -> Unit,
) {
    private val mutableState = MutableStateFlow(ArtworkLoadingState(BridgeEagerCacheFillStatus.NotRunning))
    val state: StateFlow<ArtworkLoadingState> = mutableState.asStateFlow()

    fun apply(status: BridgeEagerCacheFillStatus) {
        mutableState.update { current ->
            if (current.status == status) current else ArtworkLoadingState(status)
        }
    }

    fun dismiss() {
        mutableState.update { it.copy(dismissed = true) }
    }

    fun cancel() = cancelAction()
}
