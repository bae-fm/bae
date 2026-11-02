package fm.bae.app.ui.settings

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.AppSessionHolder
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.SyncFailure
import fm.bae.app.data.SyncLine
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.DeviceResetDialog
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.Notice
import fm.bae.app.ui.components.StatusTone
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.bridgeSyncIndicatorKey

/**
 * The one line sync shows: what it is doing, or "Waiting: N items". Waiting,
 * and a list that couldn't be read, open the list of what it waits on. With no
 * storage set up on this device there is no line.
 */
@Composable
internal fun SyncStatusLine(
    line: SyncLine,
    onOpenPending: () -> Unit,
) {
    val context = LocalContext.current
    val text = syncLineText(context, line) ?: return
    val waiting = line == SyncLine.Unreadable || (line as? SyncLine.Read)?.indicator is BridgeSyncIndicator.Waiting
    Row(
        modifier = Modifier.fillMaxWidth().clickable(enabled = waiting, onClick = onOpenPending),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text = text, modifier = Modifier.weight(1f))
        if (waiting) {
            Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null)
        }
    }
}

/**
 * The status line's words: core's for each state of the status it names, this app's for checking and for a read that
 * failed; none with no storage set up on this device, which has no sync to describe.
 */
internal fun syncLineText(
    context: Context,
    line: SyncLine,
): String? =
    when (line) {
        SyncLine.Checking -> context.getString(R.string.settings_sync_checking)
        SyncLine.Unreadable -> context.getString(R.string.settings_sync_unreadable)
        SyncLine.NoStorage -> null
        is SyncLine.Read -> syncIndicatorText(context, line.indicator)
    }

/** Core's words for a status as read. */
private fun syncIndicatorText(
    context: Context,
    indicator: BridgeSyncIndicator,
): String {
    val key = bridgeSyncIndicatorKey(indicator)
    return when (indicator) {
        is BridgeSyncIndicator.Waiting -> context.coreString(key, mapOf("count" to indicator.items.toLong()))
        else -> context.coreString(key)
    }
}

/**
 * The last sync failure, under the status line, with its details to copy, and Reconnect where core offers it: a start
 * failed and left sync not running.
 */
@Composable
internal fun SyncFailureNotice(
    error: SyncFailure?,
    onReconnect: () -> Unit,
) {
    if (error == null) return
    val context = LocalContext.current
    Notice(StatusTone.WARNING) {
        Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
            ErrorDetailDisclosure(error = error.error, lineColor = LocalContentColor.current)
            if (error.offersReconnect) {
                OutlinedButton(onClick = onReconnect) {
                    Text(context.coreString("core.pending.action.reconnect"))
                }
            }
        }
    }
}

/**
 * Reset Device, after its warning, where sync found another copy of this
 * device; nothing for any other failure.
 */
@Composable
internal fun DeviceResetOffer(
    session: OpenLibrary,
    error: SyncFailure?,
    onResetDevice: (BridgeDeviceReset) -> Unit,
) {
    if (error?.offersDeviceReset != true) return
    val context = LocalContext.current
    var reset by remember(session) { mutableStateOf<BridgeDeviceReset?>(null) }
    OutlinedButton(onClick = { reset = AppSessionHolder.prepareDeviceReset(context, session.libraryId) }) {
        Text(context.coreString("core.sync.reset_device.confirm"))
    }
    reset?.let { prepared ->
        DeviceResetDialog(
            reset = prepared,
            onConfirm = {
                reset = null
                onResetDevice(prepared)
            },
            onDismiss = { reset = null },
        )
    }
}

/** Which provider a location is on. */
internal fun storageProvider(location: BridgeStorageLocation): BridgeCloudProvider =
    when (location) {
        is BridgeStorageLocation.S3 -> BridgeCloudProvider.S3
        is BridgeStorageLocation.Dropbox -> BridgeCloudProvider.DROPBOX
        is BridgeStorageLocation.OneDrive -> BridgeCloudProvider.ONE_DRIVE
        is BridgeStorageLocation.CloudKit -> BridgeCloudProvider.CLOUD_KIT
    }

@Preview(showBackground = true)
@Composable
private fun SyncStatusLinePreview() {
    BaeTheme {
        SyncStatusLine(line = SyncLine.Read(BridgeSyncIndicator.Waiting(3uL)), onOpenPending = {})
    }
}
