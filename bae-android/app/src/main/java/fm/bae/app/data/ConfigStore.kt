package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeConfig

/** Library configuration mirror. The config value stream is its only writer. */
class ConfigStore(
    initialConfig: BridgeConfig,
) {
    private val _config = MutableStateFlow(initialConfig)
    val config: StateFlow<BridgeConfig> = _config.asStateFlow()

    fun setConfig(config: BridgeConfig) {
        _config.value = config
    }
}
