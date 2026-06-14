package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeConfig

/**
 * Library configuration mirror. Holds the latest [BridgeConfig] plus the
 * sync-loop run status, the sync-loop error state, and a transient app-level
 * error. The [fm.bae.app.data.UiEventReducer] is the sole writer; views observe
 * the flows.
 */
class ConfigStore(initialConfig: BridgeConfig, initialSyncReady: Boolean) {
    private val _config = MutableStateFlow(initialConfig)
    val config: StateFlow<BridgeConfig> = _config.asStateFlow()

    /**
     * Whether the sync loop is running right now. Runtime status, not
     * configuration: it rides the `configChanged` event alongside `config` but
     * lands here, not on the [BridgeConfig] mirror, since it changes
     * independently of any persisted setting.
     */
    private val _syncReady = MutableStateFlow(initialSyncReady)
    val syncReady: StateFlow<Boolean> = _syncReady.asStateFlow()

    /** Latest sync-loop error message; null clears a prior failure. */
    private val _syncError = MutableStateFlow<String?>(null)
    val syncError: StateFlow<String?> = _syncError.asStateFlow()

    /** Transient app-level error surfaced by `Error` / `ErrorCleared` events. */
    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    fun setConfig(config: BridgeConfig) {
        _config.value = config
    }

    fun setSyncReady(ready: Boolean) {
        _syncReady.value = ready
    }

    fun setSyncError(message: String?) {
        _syncError.value = message
    }

    fun showError(message: String) {
        _error.value = message
    }

    fun clearError() {
        _error.value = null
    }
}
