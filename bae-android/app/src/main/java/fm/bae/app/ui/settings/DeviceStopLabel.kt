package fm.bae.app.ui.settings

import fm.bae.app.R
import uniffi.bae_bridge.BridgeDeviceStop

/** Why this device stopped for good, ending a change it asked for before the change was made, as a sentence. */
internal fun BridgeDeviceStop.labelRes(): Int =
    when (this) {
        BridgeDeviceStop.REMOVED -> R.string.device_stop_removed
        BridgeDeviceStop.REPLACED -> R.string.device_stop_replaced
        BridgeDeviceStop.COPIED -> R.string.device_stop_copied
        BridgeDeviceStop.MEMBER_KEYS_REPLACED -> R.string.device_stop_member_keys_replaced
    }
