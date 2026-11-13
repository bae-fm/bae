package fm.bae.app.data

import android.content.Context
import fm.bae.app.DisplayError
import fm.bae.app.coreString
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeRemotePlayback

/**
 * Cast state for the playback surfaces: where playback is going (this device,
 * a renderer, or back here after a renderer's link was lost), and the
 * receivers the picker lists, the one cast to apart from the rest. Both follow
 * retained value subscriptions.
 */
class CastStore {
    private val _status = MutableStateFlow<BridgeRemotePlayback>(BridgeRemotePlayback.Local)
    val status: StateFlow<BridgeRemotePlayback> = _status.asStateFlow()

    private val _receivers = MutableStateFlow(BridgeCastReceiverList(casting = null, found = emptyList()))
    val receivers: StateFlow<BridgeCastReceiverList> = _receivers.asStateFlow()

    /**
     * Apply the retained status, which every playback value repeats. Returns
     * the cast it newly reports ended — a link lost since the last status —
     * so the person is told once, not on every value.
     */
    fun applyStatus(remote: BridgeRemotePlayback): BridgeRemotePlayback.Ended? {
        val previous = _status.value
        _status.value = remote
        return (remote as? BridgeRemotePlayback.Ended)?.takeIf { it != previous }
    }

    fun setReceivers(receivers: BridgeCastReceiverList) {
        _receivers.value = receivers
    }
}

/** Whether the list holds no receiver: none cast to, and none found. */
val BridgeCastReceiverList.isEmpty: Boolean
    get() = casting == null && found.isEmpty()

/** The device playback is on, else null. */
fun castingDevice(status: BridgeRemotePlayback): BridgeCastReceiver? =
    when (status) {
        is BridgeRemotePlayback.On -> status.device
        BridgeRemotePlayback.Local, is BridgeRemotePlayback.Ended -> null
    }

/**
 * The name of the receiver playback is on when it takes no volume from bae, else
 * null: the volume controls then give way to a line saying where it is set.
 */
fun receiverWithoutVolume(status: BridgeRemotePlayback): String? {
    val device = castingDevice(status) ?: return null
    return if (device.takesVolume) null else device.name
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
