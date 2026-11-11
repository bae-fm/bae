package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cast
import androidx.compose.material.icons.filled.CastConnected
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.CastStart
import fm.bae.app.data.castEndedFailure
import fm.bae.app.data.castingDevice
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.StatusTone
import uniffi.bae_bridge.BridgeBrowsingFor

/** The now-playing bar's cast control, absent when casting is turned off. */
@Composable
fun CastButton(session: OpenLibrary) {
    val config by session.configStore.config.collectAsState()
    if (!config.castEnabled) {
        return
    }
    val status by session.castStore.status.collectAsState()
    val castingTo = castingDevice(status)
    val endedFailure = LocalContext.current.castEndedFailure(status)
    var pickerOpen by remember { mutableStateOf(false) }

    if (pickerOpen) {
        // The picker asks for discovery while it is open, and a browse clears the list as it starts.
        DisposableEffect(Unit) {
            session.cast.startDiscovery(BridgeBrowsingFor.PICKER)
            onDispose {
                session.cast.stopDiscovery(BridgeBrowsingFor.PICKER)
                session.castStart.clearFailure()
            }
        }
        CastPickerSheet(
            session = session,
            endedFailure = endedFailure,
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
 * The device picker: why the last device stopped when its link was lost, then the receivers as they
 * answer, the one cast to first with Disconnect, or a searching line while none have.
 */
@Composable
private fun CastPickerSheet(
    session: OpenLibrary,
    endedFailure: DisplayError?,
    onDismiss: () -> Unit,
) {
    val receivers by session.castStore.receivers.collectAsState()
    val connecting by session.castStart.connecting.collectAsState()
    val castFailure by session.castStart.failure.collectAsState()
    val started by session.castStart.started.collectAsState()
    // The start this sheet began: a sheet opened later has none, so it stays open for an earlier one.
    var asked by remember { mutableStateOf<CastStart.Attempt?>(null) }
    LaunchedEffect(started) {
        if (session.castStart.didStart(asked)) onDismiss()
    }

    AndroidSheet(title = stringResource(R.string.cast), onDismiss = onDismiss) {
        PickerFailures(endedFailure = endedFailure, castFailure = castFailure)
        CastReceiverRows(
            place = CastReceiverPlace.PICKER,
            receivers = receivers,
            connectingName = connecting?.name,
            onCast = { receiver -> asked = session.castStart.cast(receiver) },
            onDisconnect = {
                session.cast.stopCasting()
                onDismiss()
            },
            looking = stringResource(R.string.cast_looking_for_devices),
        )
    }
}

/** Why the last device stopped, when its link was lost, and why the last cast this picker started failed. */
@Composable
private fun PickerFailures(
    endedFailure: DisplayError?,
    castFailure: DisplayError?,
) {
    endedFailure?.let { failure ->
        ErrorNotice(
            error = failure,
            tone = StatusTone.WARNING,
            modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        )
    }
    castFailure?.let { failure ->
        ErrorNotice(
            error = failure,
            modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        )
    }
}
