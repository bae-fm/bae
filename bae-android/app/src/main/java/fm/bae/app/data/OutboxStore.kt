package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeOutboxPauseState
import uniffi.bae_bridge.BridgeOutboxSnapshot

/**
 * Mirror of core's cloud-upload outbox snapshot. Core re-emits the full snapshot
 * on every change to the upload pipeline — including each pause/resume — so the
 * Settings pause toggle renders its state from [BridgeOutboxSnapshot.pauseState]
 * rather than tracking it optimistically.
 */
class OutboxStore(
    initialSnapshot: BridgeOutboxSnapshot,
) {
    private val _snapshot = MutableStateFlow(initialSnapshot)
    val snapshot: StateFlow<BridgeOutboxSnapshot> = _snapshot.asStateFlow()

    fun setSnapshot(snapshot: BridgeOutboxSnapshot) {
        _snapshot.value = snapshot
    }
}

internal val BridgeOutboxSnapshot.pauseRequested: Boolean
    get() = pauseState != BridgeOutboxPauseState.RUNNING
