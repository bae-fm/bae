package fm.bae.app.ui.settings

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.SyncFailure
import fm.bae.app.localizedLine
import fm.bae.app.reconnectFailedSync
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.LabeledValueRow
import fm.bae.app.ui.components.Notice
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.library.LibraryDestination
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.bridgeSyncIndicatorKey

/**
 * Where the library is stored and how its sync stands: the cloud home's
 * details, the one status line, upload pause and disconnect — or, without a
 * cloud home, the way to set one up.
 */
@Composable
internal fun SettingsSyncSection(
    onResetDevice: (BridgeDeviceReset) -> Unit,
    session: OpenLibrary,
    cloudHome: CloudHome,
    onPush: (LibraryDestination) -> Unit,
    onCloudHomeChanged: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_sync))
        when (cloudHome) {
            CloudHome.Loading -> {
                Unit
            }

            is CloudHome.Failed -> {
                ErrorText(cloudHome.message)
            }

            CloudHome.None -> {
                Text(stringResource(R.string.settings_local_only))
                Text(
                    text = stringResource(R.string.settings_set_up_cloud_home_explanation),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                PrimaryButton(onClick = { onPush(LibraryDestination.StorageSetup) }) {
                    Text(stringResource(R.string.storage_setup_title))
                }
            }

            is CloudHome.At -> {
                CloudHomeRows(cloudHome.location)
                SyncStatus(
                    session = session,
                    onOpenPending = { onPush(LibraryDestination.SyncPending) },
                    onResetDevice = onResetDevice,
                )
                SyncUploadPauseControl(session)
                DisconnectControl(session = session, onDisconnected = onCloudHomeChanged)
            }
        }
    }
}

/**
 * The status line and, under it, the last failure with Reconnect where
 * reconnecting is the remedy, or Reset Device, after its warning, where sync
 * found another copy of this device.
 */
@Composable
private fun SyncStatus(
    session: OpenLibrary,
    onOpenPending: () -> Unit,
    onResetDevice: (BridgeDeviceReset) -> Unit,
) {
    val scope = rememberCoroutineScope()
    val indicator by session.syncStatusStore.indicator.collectAsState()
    val error by session.syncStatusStore.error.collectAsState()
    SyncStatusLine(indicator = indicator, onOpenPending = onOpenPending)
    SyncFailureNotice(
        indicator = indicator,
        error = error,
        onReconnect = { scope.launch { reconnectFailedSync(session.appHandle) } },
    )
    DeviceResetOffer(session = session, error = error, onResetDevice = onResetDevice)
}

/**
 * The cloud home's provider and, for S3, the bucket it names; Dropbox and
 * OneDrive show the signed-in account, since their folder has no name to show.
 */
@Composable
private fun CloudHomeRows(location: BridgeStorageLocation) {
    LabeledValueRow(stringResource(R.string.settings_provider), cloudProviderLabel(storageProvider(location)))
    when (location) {
        is BridgeStorageLocation.S3 -> {
            LabeledValueRow(stringResource(R.string.settings_bucket), location.bucket)
            LabeledValueRow(stringResource(R.string.settings_region), location.region)
            location.endpoint?.let { LabeledValueRow(stringResource(R.string.settings_endpoint), it) }
            if (location.prefix.isNotEmpty()) {
                LabeledValueRow(stringResource(R.string.storage_s3_key_prefix), location.prefix)
            }
        }

        is BridgeStorageLocation.Dropbox -> {
            location.accountLabel?.let { LabeledValueRow(stringResource(R.string.settings_account), it) }
        }

        is BridgeStorageLocation.OneDrive -> {
            location.accountLabel?.let { LabeledValueRow(stringResource(R.string.settings_account), it) }
        }

        is BridgeStorageLocation.CloudKit -> {
            Unit
        }
    }
}

/** Disconnect, after a confirmation that names what this device could no longer play. */
@Composable
private fun DisconnectControl(
    session: OpenLibrary,
    onDisconnected: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val flow = rememberDisconnectSyncFlow(session, onDisconnected)
    val flowState by flow.state.collectAsState()

    flowState.error?.let { error ->
        ErrorText(error)
    }

    OutlinedButton(
        onClick = { flow.promptDisconnect() },
        colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.error),
    ) {
        Text(stringResource(R.string.settings_disconnect))
    }

    if (flowState.confirming) {
        DisconnectConfirmDialog(
            extraWarning = flowState.extraWarning,
            // Dismiss first so the disconnect, which can wait on a running sync,
            // fires once.
            onConfirm = {
                flow.dismissConfirm()
                scope.launch { flow.confirm() }
            },
            onDismiss = { flow.dismissConfirm() },
        )
    }
}

@Composable
private fun DisconnectConfirmDialog(
    extraWarning: String?,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.settings_disconnect_confirm_title)) },
        text = { Text(disconnectConfirmMessage(stringResource(R.string.settings_disconnect_body), extraWarning)) },
        confirmButton = {
            TextButton(onClick = onConfirm) { Text(stringResource(R.string.settings_disconnect)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

@Composable
private fun rememberDisconnectSyncFlow(
    session: OpenLibrary,
    onDisconnected: () -> Unit,
): DisconnectSyncFlow {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    return remember(session) {
        DisconnectSyncFlow(
            scope = scope,
            cloudOnlyReleaseCount = { session.appHandle.cloudOnlyReleaseCount() },
            disconnect = {
                session.appHandle.disconnectCloudProvider()
                onDisconnected()
            },
            strings =
                DisconnectStrings(
                    atRiskLine = { count ->
                        context.coreString(
                            "core.sync.cloud_only_releases",
                            mapOf("count" to count.toLong()),
                        )
                    },
                    // No detail means a cancellation, so leave the error clear.
                    warningFailedLine = { e ->
                        disconnectErrorDetail(context, e)?.let {
                            context.getString(R.string.settings_disconnect_warning_check_failed, it)
                        }
                    },
                    disconnectFailedLine = { e ->
                        disconnectErrorDetail(context, e)?.let {
                            context.getString(R.string.settings_disconnect_failed, it)
                        }
                    },
                ),
        )
    }
}

private fun disconnectErrorDetail(
    context: Context,
    error: Throwable,
): String? =
    when (error) {
        is BridgeException -> context.localizedLine(error)
        else -> error.message ?: error::class.java.simpleName
    }

@Preview(showBackground = true)
@Composable
private fun CloudHomeRowsPreview() {
    BaeTheme {
        Column {
            CloudHomeRows(
                BridgeStorageLocation.S3(
                    bucket = "household-music",
                    region = "auto",
                    endpoint = "https://account-id.r2.cloudflarestorage.com",
                    profile = null,
                    prefix = "",
                ),
            )
        }
    }
}
