package fm.bae.app.ui.settings

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidDisclosure
import fm.bae.app.ui.components.AndroidRow
import uniffi.bae_bridge.BridgeFormerDevice
import uniffi.bae_bridge.BridgeFormerDeviceState

/**
 * A person's removed and replaced devices behind a card row that expands them
 * into rows of the same card, closed at first. They no longer have access, so
 * they offer no actions.
 */
@Composable
internal fun FormerDevices(devices: List<BridgeFormerDevice>) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    AndroidDisclosure(
        label = pluralStringResource(R.plurals.members_former_devices, devices.size, devices.size),
        open = expanded,
        onToggle = { expanded = !expanded },
    ) {
        devices.forEach { device ->
            AndroidRow(title = device.name, subtitle = stringResource(device.state.labelRes()))
        }
    }
}

private fun BridgeFormerDeviceState.labelRes(): Int =
    when (this) {
        BridgeFormerDeviceState.REMOVED -> R.string.device_state_removed
        BridgeFormerDeviceState.REPLACED -> R.string.device_state_replaced
    }
