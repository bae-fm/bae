package fm.bae.app.data

import fm.bae.app.ErrorLines
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.bridgeSyncIndicator

/**
 * Library configuration mirror. Holds the latest [BridgeConfig] plus the
 * sync-loop run status, the sync-loop error state, and a transient app-level
 * error. The event adapter refreshes query-backed fields from core; views
 * observe the flows.
 */
class ConfigStore(
    initialConfig: BridgeConfig,
    initialSyncReady: Boolean,
) {
    private val _config = MutableStateFlow(initialConfig)
    val config: StateFlow<BridgeConfig> = _config.asStateFlow()

    /**
     * Whether the sync loop is running right now. Runtime status, not
     * configuration: it is refreshed from the sync-status snapshot, not from
     * the [BridgeConfig] mirror, since it changes independently of any persisted
     * setting.
     */
    private val _syncReady = MutableStateFlow(initialSyncReady)
    val syncReady: StateFlow<Boolean> = _syncReady.asStateFlow()

    /**
     * Whether the sync loop is currently mid-cycle. Distinct from [syncReady]
     * (the loop being alive): this is true only while a cycle is actively
     * pulling, from a sync kick through to the cycle finishing. The library
     * screen shows its progress indicator while this holds.
     */
    private val _syncing = MutableStateFlow(false)
    val syncing: StateFlow<Boolean> = _syncing.asStateFlow()

    /** Latest sync-loop error message; null clears a prior failure. */
    private val _syncError = MutableStateFlow<String?>(null)
    val syncError: StateFlow<String?> = _syncError.asStateFlow()

    /**
     * The toolbar/settings badge state, decided by core (error > syncing > synced
     * > idle). The UI maps a variant to a label; it never re-derives which state
     * wins, which is how a stale timestamp used to read as "Synced".
     */
    private val _syncIndicator = MutableStateFlow<BridgeSyncIndicator>(BridgeSyncIndicator.Idle)
    val syncIndicator: StateFlow<BridgeSyncIndicator> = _syncIndicator.asStateFlow()

    /** Transient app-level error surfaced by `Error` events. */
    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    fun setConfig(config: BridgeConfig) {
        _config.value = config
    }

    fun setSyncReady(syncReady: Boolean) {
        _syncReady.value = syncReady
    }

    fun setSyncStatus(
        status: BridgeSyncStatusSnapshot,
        errors: ErrorLines,
    ) {
        _syncReady.value = status.syncReady
        _syncing.value = status.syncing
        _syncError.value = status.error?.let { errors.line(it) }
        _syncIndicator.value = bridgeSyncIndicator(status)
    }

    /**
     * Surface an error. A null line means core says there is nothing to show — a
     * cancellation — so the banner is left alone rather than raised empty.
     */
    fun showError(message: String?) {
        if (message == null) {
            return
        }
        _error.value = message
    }
}
