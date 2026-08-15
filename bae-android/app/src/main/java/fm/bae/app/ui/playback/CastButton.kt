package fm.bae.app.ui.playback

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cast
import androidx.compose.material.icons.filled.CastConnected
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Speaker
import androidx.compose.material.icons.filled.Tv
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.castingDeviceName
import fm.bae.app.localizedLine
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeCastDevice
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeRendererKind

private val logger = BaeLogger("bae.CastButton")

/**
 * The now-playing bar's cast control: a glyph that opens the device picker.
 * Browsing runs only while the picker is up. Absent entirely when casting is
 * turned off — core browses nothing then and refuses a session, so there is
 * nothing for the control to do.
 */
@Composable
fun CastButton(session: OpenLibrary) {
    val config by session.configStore.config.collectAsState()
    if (!config.castEnabled) {
        return
    }
    val status by session.castStore.status.collectAsState()
    val castingTo = castingDeviceName(status)
    var pickerOpen by remember { mutableStateOf(false) }

    if (pickerOpen) {
        // Browsing is not always-on: it runs with the picker, and core clears
        // the list as it starts, so the sheet opens on what this browse finds.
        DisposableEffect(Unit) {
            session.cast.startDiscovery()
            onDispose { session.cast.stopDiscovery() }
        }
        CastPickerSheet(
            session = session,
            castingTo = castingTo,
            onDismiss = { pickerOpen = false },
        )
    }

    IconButton(onClick = { pickerOpen = true }) {
        Icon(
            imageVector = if (castingTo == null) Icons.Filled.Cast else Icons.Filled.CastConnected,
            contentDescription = stringResource(R.string.cast),
            tint =
                if (castingTo == null) {
                    MaterialTheme.colorScheme.onSurfaceVariant
                } else {
                    MaterialTheme.colorScheme.primary
                },
        )
    }
}

/**
 * The device picker: the active-casting row when casting, then the discovered
 * devices, or an empty-state line while none have answered.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CastPickerSheet(
    session: OpenLibrary,
    castingTo: String?,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var castJob by remember { mutableStateOf<Job?>(null) }
    val devices by session.castStore.devices.collectAsState()
    val sheetState = rememberModalBottomSheetState()
    val bottomInset = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()

    DisposableEffect(Unit) {
        onDispose { castJob?.cancel() }
    }

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheetState) {
        Column(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .padding(PaddingValues(bottom = bottomInset + 16.dp)),
        ) {
            Text(
                text = stringResource(R.string.cast),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
            )
            CastDeviceRows(
                devices = devices,
                castingTo = castingTo,
                onDisconnect = {
                    session.cast.stopCasting()
                    onDismiss()
                },
                onCast = { device ->
                    castJob?.cancel()
                    castJob =
                        scope.launch {
                            try {
                                session.cast.castTo(device.id)
                                onDismiss()
                            } catch (e: BridgeException) {
                                logger.error("Failed to cast to ${device.id}", e)
                                session.configStore.showError(context.localizedLine(e))
                            }
                        }
                },
            )
        }
    }
}

@Composable
private fun CastDeviceRows(
    devices: List<BridgeCastDevice>,
    castingTo: String?,
    onDisconnect: () -> Unit,
    onCast: (BridgeCastDevice) -> Unit,
) {
    if (castingTo != null) {
        CastingRow(deviceName = castingTo, onDisconnect = onDisconnect)
    }
    if (devices.isEmpty()) {
        Text(
            text = stringResource(R.string.cast_no_devices),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
        )
    } else {
        devices.forEach { device ->
            DeviceRow(
                device = device,
                isActive = device.name == castingTo,
                onCast = { onCast(device) },
            )
        }
    }
}

@Composable
private fun CastingRow(
    deviceName: String,
    onDisconnect: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = Icons.Filled.CastConnected,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.primary,
        )
        Spacer(modifier = Modifier.width(12.dp))
        Text(
            text = stringResource(R.string.cast_casting_to, deviceName),
            style = MaterialTheme.typography.bodyMedium,
            maxLines = 1,
            modifier = Modifier.weight(1f),
        )
        TextButton(onClick = onDisconnect) {
            Text(stringResource(R.string.cast_disconnect))
        }
    }
}

@Composable
private fun DeviceRow(
    device: BridgeCastDevice,
    isActive: Boolean,
    onCast: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onCast)
                .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = deviceIcon(device.kind),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(modifier = Modifier.width(12.dp))
        Text(
            text = device.name,
            style = MaterialTheme.typography.bodyMedium,
            maxLines = 1,
            modifier = Modifier.weight(1f),
        )
        if (isActive) {
            Icon(
                imageVector = Icons.Filled.Check,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

/**
 * A flavor hint for a row: a cast glyph for Cast, a speaker for an AirPlay
 * receiver, a TV for a UPnP renderer (commonly a TV or AV receiver). The list
 * itself isn't segregated by protocol — a speaker is a speaker.
 */
private fun deviceIcon(kind: BridgeRendererKind): ImageVector =
    when (kind) {
        BridgeRendererKind.CAST -> Icons.Filled.Cast
        BridgeRendererKind.AIR_PLAY -> Icons.Filled.Speaker
        BridgeRendererKind.DLNA -> Icons.Filled.Tv
    }
