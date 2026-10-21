package fm.bae.app.data

import android.content.Context
import fm.bae.app.DisplayError
import fm.bae.app.coreString
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeCastDevice
import uniffi.bae_bridge.BridgeRemoteDevice
import uniffi.bae_bridge.BridgeRemotePlayback

/**
 * Cast state for the playback surfaces: where playback is going (this device,
 * a renderer, or back here after a renderer's link was lost), and what the
 * picker found while it was open. Both follow retained value subscriptions.
 */
class CastStore {
    private val _status = MutableStateFlow<BridgeRemotePlayback>(BridgeRemotePlayback.Local)
    val status: StateFlow<BridgeRemotePlayback> = _status.asStateFlow()

    private val _devices = MutableStateFlow<List<BridgeCastDevice>>(emptyList())
    val devices: StateFlow<List<BridgeCastDevice>> = _devices.asStateFlow()

    /** Apply the retained status. */
    fun applyStatus(remote: BridgeRemotePlayback) {
        _status.value = remote
    }

    fun setDevices(devices: List<BridgeCastDevice>) {
        _devices.value = devices
    }
}

/**
 * The device playback is on, else null. The picker marks the device whose id
 * matches; device names need not be unique.
 */
fun castingDevice(status: BridgeRemotePlayback): BridgeRemoteDevice? =
    when (status) {
        is BridgeRemotePlayback.On -> status.device
        BridgeRemotePlayback.Local, is BridgeRemotePlayback.Ended -> null
    }

/**
 * The casting device's name — what drives the cast button's active state, the
 * "Casting to …" row, and the settings confirmation.
 */
fun castingDeviceName(status: BridgeRemotePlayback): String? = castingDevice(status)?.name

/**
 * Why the last device stopped playing, when its link was lost: the line names
 * the device, the detail is core's reason for the disclosure.
 */
fun Context.castEndedFailure(status: BridgeRemotePlayback): DisplayError? =
    (status as? BridgeRemotePlayback.Ended)?.let { ended ->
        DisplayError(coreString("core.cast.ended", mapOf("device" to ended.device.name)), ended.detail)
    }
