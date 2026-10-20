package fm.bae.app.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.semantics
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import uniffi.bae_bridge.BridgeFormerDevice
import uniffi.bae_bridge.BridgeFormerDeviceState

/**
 * A person's removed and replaced devices behind a row that expands them,
 * closed at first. They no longer have access, so they offer no actions.
 */
@Composable
internal fun FormerDevices(devices: List<BridgeFormerDevice>) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    Column(modifier = Modifier.padding(start = ThemeSpace.edge)) {
        Row(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .clickable(role = Role.Button) { expanded = !expanded }
                    .semantics {
                        if (expanded) {
                            collapse {
                                expanded = false
                                true
                            }
                        } else {
                            expand {
                                expanded = true
                                true
                            }
                        }
                    }.padding(vertical = ThemeSpace.inline),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = pluralStringResource(R.plurals.members_former_devices, devices.size, devices.size),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.weight(1f),
            )
            Icon(
                imageVector = if (expanded) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (expanded) {
            devices.forEach { device ->
                Column(modifier = Modifier.padding(start = ThemeSpace.edge, top = ThemeSpace.inline)) {
                    Text(text = device.name, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(
                        text = stringResource(device.state.labelRes()),
                        style = ThemeText.detail.style,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

private fun BridgeFormerDeviceState.labelRes(): Int =
    when (this) {
        BridgeFormerDeviceState.REMOVED -> R.string.device_state_removed
        BridgeFormerDeviceState.REPLACED -> R.string.device_state_replaced
    }
