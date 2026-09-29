package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeCastDevice
import uniffi.bae_bridge.BridgeCastStatus
import uniffi.bae_bridge.BridgeRemoteDevice

/**
 * Cast state for the playback surfaces: which device playback is on, and what
 * the picker found while it was open. Both follow retained value subscriptions.
 */
class CastStore {
    private val _status = MutableStateFlow<BridgeCastStatus>(BridgeCastStatus.NotCasting)
    val status: StateFlow<BridgeCastStatus> = _status.asStateFlow()

    private val _devices = MutableStateFlow<List<BridgeCastDevice>>(emptyList())
    val devices: StateFlow<List<BridgeCastDevice>> = _devices.asStateFlow()

    /** Apply the retained status: the device while casting, null on local output. */
    fun applyStatus(device: BridgeRemoteDevice?) {
        _status.value =
            device?.let { BridgeCastStatus.Casting(it) } ?: BridgeCastStatus.NotCasting
    }

    fun setDevices(devices: List<BridgeCastDevice>) {
        _devices.value = devices
    }
}

/**
 * The device playback is on, else null. The picker marks the device whose id
 * matches; device names need not be unique.
 */
fun castingDevice(status: BridgeCastStatus): BridgeRemoteDevice? =
    when (status) {
        is BridgeCastStatus.Casting -> status.device
        BridgeCastStatus.NotCasting -> null
    }

/**
 * The casting device's name — what drives the cast button's active state, the
 * "Casting to …" row, and the settings confirmation.
 */
fun castingDeviceName(status: BridgeCastStatus): String? = castingDevice(status)?.name
