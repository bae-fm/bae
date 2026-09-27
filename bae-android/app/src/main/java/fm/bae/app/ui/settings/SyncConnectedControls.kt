package fm.bae.app.ui.settings

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
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
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeText
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeBlockedSyncOperation
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeSyncConfig
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncProvider

/** The sync row's state; an error takes precedence over readiness. */
internal sealed interface SettingsSyncStatus {
    data class Failed(
        val error: SyncFailure?,
    ) : SettingsSyncStatus

    data object Synced : SettingsSyncStatus

    data object Syncing : SettingsSyncStatus
}

internal fun settingsSyncStatus(
    indicator: BridgeSyncIndicator,
    syncError: SyncFailure?,
): SettingsSyncStatus =
    when (indicator) {
        // Blocked operations also raise Error; their own rows carry their failures.
        is BridgeSyncIndicator.Error -> SettingsSyncStatus.Failed(syncError)

        is BridgeSyncIndicator.Synced -> SettingsSyncStatus.Synced

        is BridgeSyncIndicator.Syncing, is BridgeSyncIndicator.Idle -> SettingsSyncStatus.Syncing
    }

/**
 * Provider details, sync status, upload pause, and disconnect; after a disconnect
 * core clears the sync config, which removes this block.
 */
@Composable
internal fun SyncConnectedControls(
    session: OpenLibrary,
    sync: BridgeSyncConfig,
    indicator: BridgeSyncIndicator,
    syncError: SyncFailure?,
    blocked: List<BridgeBlockedSyncOperation>,
) {
    val scope = rememberCoroutineScope()
    val flow = rememberDisconnectSyncFlow(session)
    val flowState by flow.state.collectAsState()

    SyncProviderRows(sync = sync)
    SettingsSyncStatusRow(
        indicator = indicator,
        syncError = syncError,
        onReconnect = {
            scope.launch { reconnectFailedSync(session.appHandle) }
        },
    )
    BlockedSyncOperations(
        operations = blocked,
        onRetry = { session.appHandle.retryBlockedSyncOperation(it) },
    )

    SyncUploadPauseControl(session)

    flowState.error?.let { error ->
        Text(
            text = error,
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.error,
        )
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

/** The sync status line: a failure with its message and reconnect action, or synced/syncing. */
@Composable
internal fun SettingsSyncStatusRow(
    indicator: BridgeSyncIndicator,
    syncError: SyncFailure?,
    onReconnect: () -> Unit,
) {
    when (val status = settingsSyncStatus(indicator, syncError)) {
        is SettingsSyncStatus.Failed -> {
            status.error?.let { error ->
                if (error.canReconnect) {
                    Text(
                        text = stringResource(R.string.settings_sync_disconnected),
                        style = ThemeText.body.style,
                        color = BaeTheme.colors.warning,
                    )
                }
                SyncStatusDetail(error.message)
                if (error.canReconnect) {
                    OutlinedButton(onClick = onReconnect) {
                        Text(stringResource(R.string.settings_reconnect))
                    }
                }
            }
        }

        SettingsSyncStatus.Synced -> {
            SyncStatusDetail(stringResource(R.string.settings_synced))
        }

        SettingsSyncStatus.Syncing -> {
            SyncStatusDetail(stringResource(R.string.settings_syncing))
        }
    }
}

@Composable
private fun SyncStatusDetail(text: String) {
    Text(
        text = text,
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun DisconnectConfirmDialog(
    extraWarning: String?,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val base =
        stringResource(R.string.settings_disconnect_body) +
            " " +
            stringResource(R.string.settings_disconnect_reconnect_hint)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.settings_disconnect_confirm_title)) },
        text = { Text(disconnectConfirmMessage(base, extraWarning)) },
        confirmButton = {
            TextButton(onClick = onConfirm) { Text(stringResource(R.string.settings_disconnect)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/** Provider name, account, and for S3 the bucket, region, and endpoint. */
@Composable
private fun SyncProviderRows(sync: BridgeSyncConfig) {
    LabeledSettingRow(stringResource(R.string.settings_provider), syncProviderLabel(sync.provider))
    sync.cloudAccountDisplay?.let { account ->
        LabeledSettingRow(stringResource(R.string.settings_account), account)
    }
    val provider = sync.provider
    if (provider is BridgeSyncProvider.S3) {
        provider.bucket?.let { LabeledSettingRow(stringResource(R.string.settings_bucket), it) }
        provider.region?.let { LabeledSettingRow(stringResource(R.string.settings_region), it) }
        // An empty endpoint means the AWS default — render no row rather than a blank value.
        provider.endpoint?.takeIf { it.isNotEmpty() }?.let {
            LabeledSettingRow(stringResource(R.string.settings_endpoint), it)
        }
    }
}

@Composable
private fun LabeledSettingRow(
    label: String,
    value: String,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text = label)
        Text(text = value, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun rememberDisconnectSyncFlow(session: OpenLibrary): DisconnectSyncFlow {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    return remember(session) {
        DisconnectSyncFlow(
            scope = scope,
            cloudOnlyReleaseCount = { session.appHandle.cloudOnlyReleaseCount() },
            disconnect = { session.appHandle.disconnectCloudProvider() },
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

@Composable
private fun syncProviderLabel(provider: BridgeSyncProvider): String =
    when (provider) {
        is BridgeSyncProvider.S3 -> stringResource(R.string.cloud_provider_s3)
        BridgeSyncProvider.GoogleDrive -> stringResource(R.string.cloud_provider_google_drive)
        BridgeSyncProvider.Dropbox -> stringResource(R.string.cloud_provider_dropbox)
        BridgeSyncProvider.OneDrive -> stringResource(R.string.cloud_provider_onedrive)
        BridgeSyncProvider.CloudKit -> stringResource(R.string.cloud_provider_icloud)
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
private fun SyncProviderRowsPreview() {
    BaeTheme {
        Column {
            SyncProviderRows(sync = PreviewData.syncConfig())
        }
    }
}
