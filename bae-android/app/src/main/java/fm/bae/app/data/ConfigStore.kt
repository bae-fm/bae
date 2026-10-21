package fm.bae.app.data

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeConfig

/**
 * Library configuration mirror. The config value stream is its only writer.
 * Transient app errors remain here because the library screen owns their
 * presentation.
 */
class ConfigStore(
    initialConfig: BridgeConfig,
) {
    private val _config = MutableStateFlow(initialConfig)
    val config: StateFlow<BridgeConfig> = _config.asStateFlow()

    /** Transient app-level error surfaced by `Error` events. */
    private val _error = MutableStateFlow<DisplayError?>(null)
    val error: StateFlow<DisplayError?> = _error.asStateFlow()

    fun setConfig(config: BridgeConfig) {
        _config.value = config
    }

    /**
     * Surface an error. Null means core says there is nothing to show — a
     * cancellation — so the banner is left alone rather than raised empty.
     */
    fun showError(error: DisplayError?) {
        if (error == null) {
            return
        }
        _error.value = error
    }
}
