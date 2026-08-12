package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeCastDevice
import uniffi.bae_bridge.BridgeCastStatus

/**
 * Cast state for the playback surfaces: which device playback is on, and what
 * the picker found while it was open. The status follows core's
 * `CastStatusChanged` event (so a receiver-side end lands here too); the device
 * list follows the cast-device subscription.
 */
class CastStore {
    private val _status = MutableStateFlow<BridgeCastStatus>(BridgeCastStatus.NotCasting)
    val status: StateFlow<BridgeCastStatus> = _status.asStateFlow()

    private val _devices = MutableStateFlow<List<BridgeCastDevice>>(emptyList())
    val devices: StateFlow<List<BridgeCastDevice>> = _devices.asStateFlow()

    /** Apply a status event: a name while casting, null back on local output. */
    fun applyStatus(deviceName: String?) {
        _status.value =
            deviceName?.let { BridgeCastStatus.Casting(it) } ?: BridgeCastStatus.NotCasting
    }

    fun setDevices(devices: List<BridgeCastDevice>) {
        _devices.value = devices
    }
}

/**
 * The device name while casting, else null — what drives the cast button's
 * active state, the "Casting to …" row, and the settings confirmation.
 */
fun castingDeviceName(status: BridgeCastStatus): String? =
    when (status) {
        is BridgeCastStatus.Casting -> status.deviceName
        BridgeCastStatus.NotCasting -> null
    }
