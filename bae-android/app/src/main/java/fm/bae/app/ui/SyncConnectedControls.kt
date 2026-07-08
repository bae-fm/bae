package fm.bae.app.ui

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
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
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.localizedLine
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeSyncConfig
import uniffi.bae_bridge.BridgeSyncProvider

/**
 * The sync row's rendered state, derived from the runtime sync snapshot. A
 * present error means the last sync cycle failed and the row offers a
 * reconnect; an absent error falls back to whether the loop is up ([Synced]) or
 * still coming up ([Syncing]). Only meaningful when a cloud provider is
 * configured — the error takes precedence over readiness.
 */
internal sealed interface SettingsSyncStatus {
    data class Disconnected(
        val error: String,
    ) : SettingsSyncStatus

    data object Synced : SettingsSyncStatus

    data object Syncing : SettingsSyncStatus
}

internal fun settingsSyncStatus(
    syncError: String?,
    syncReady: Boolean,
): SettingsSyncStatus =
    when {
        syncError != null -> SettingsSyncStatus.Disconnected(syncError)
        syncReady -> SettingsSyncStatus.Synced
        else -> SettingsSyncStatus.Syncing
    }

/**
 * The connected-provider controls: the provider details, the live sync-status
 * row, an upload-pause toggle, and a destructive disconnect action with its
 * confirmation. Rendered only when a cloud provider is configured. After a
 * successful disconnect, core clears the sync config and re-emits sync status,
 * so this whole block falls away through the config/sync-status invalidations —
 * no local cleanup here.
 */
@Composable
internal fun SyncConnectedControls(
    session: OpenLibrary,
    sync: BridgeSyncConfig,
    syncReady: Boolean,
    syncError: String?,
    ioDispatcher: CoroutineDispatcher,
) {
    val scope = rememberCoroutineScope()
    val outbox by session.outboxStore.snapshot.collectAsState()
    val flow = rememberDisconnectSyncFlow(session, ioDispatcher)
    val flowState by flow.state.collectAsState()

    SyncProviderRows(sync = sync)
    SettingsSyncStatusRow(
        syncError = syncError,
        syncReady = syncReady,
        onReconnect = { session.appHandle.triggerSync() },
    )

    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(stringResource(R.string.settings_pause_uploads), modifier = Modifier.weight(1f))
        Switch(
            checked = outbox.paused,
            onCheckedChange = { paused -> scope.launch { session.appHandle.setSyncPaused(paused) } },
        )
    }
    Text(
        text = stringResource(R.string.settings_pause_uploads_footer),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )

    flowState.error?.let { error ->
        Text(
            text = error,
            style = MaterialTheme.typography.bodySmall,
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
            // Dismiss before the disconnect runs so the destructive action fires
            // once: the call can block for an in-flight sync cycle, and the dialog
            // is gone before it returns (matching the confirm-then-detach flow).
            onConfirm = {
                flow.dismissConfirm()
                scope.launch { flow.confirm() }
            },
            onDismiss = { flow.dismissConfirm() },
        )
    }
}

/**
 * The sync status line under "Cloud sync on": either a failure with its message
 * and a reconnect action, or the live loop's synced/coming-up state. Rendered
 * only when a cloud provider is configured.
 */
@Composable
private fun SettingsSyncStatusRow(
    syncError: String?,
    syncReady: Boolean,
    onReconnect: () -> Unit,
) {
    when (val status = settingsSyncStatus(syncError, syncReady)) {
        is SettingsSyncStatus.Disconnected -> {
            Text(
                text = stringResource(R.string.settings_sync_disconnected),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
            )
            SyncStatusDetail(status.error)
            OutlinedButton(onClick = onReconnect) {
                Text(stringResource(R.string.settings_reconnect))
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
        style = MaterialTheme.typography.bodySmall,
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

/**
 * The configured provider's identity rows: provider name, account, and — for
 * S3 — the bucket/region/endpoint the library's cloud home lives at.
 */
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
private fun rememberDisconnectSyncFlow(
    session: OpenLibrary,
    ioDispatcher: CoroutineDispatcher,
): DisconnectSyncFlow {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    return remember(session) {
        DisconnectSyncFlow(
            scope = scope,
            warningMessage = { session.appHandle.disconnectWarningMessage() },
            disconnect = { session.appHandle.disconnectCloudProvider() },
            warningFailedLine = { e ->
                context.getString(
                    R.string.settings_disconnect_warning_check_failed,
                    disconnectErrorDetail(context, e),
                )
            },
            disconnectFailedLine = { e ->
                context.getString(R.string.settings_disconnect_failed, disconnectErrorDetail(context, e))
            },
            ioDispatcher = ioDispatcher,
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
): String =
    when (error) {
        is BridgeException -> context.localizedLine(error)
        else -> error.message ?: error::class.java.simpleName
    }
