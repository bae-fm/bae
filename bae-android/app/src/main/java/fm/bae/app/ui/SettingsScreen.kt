package fm.bae.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import uniffi.bae_bridge.BridgeConfig

/**
 * Minimal per-device settings: the library's sync status, and a destructive
 * action to remove the library from this device. v1 mobile is read-only
 * otherwise. Reached from the gear in the library top bar.
 */
@Composable
fun SettingsScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
    onLeaveLibrary: () -> Unit,
) {
    val config by session.configStore.config.collectAsState()
    val syncReady by session.configStore.syncReady.collectAsState()
    var confirmLeave by remember { mutableStateOf(false) }

    Column(modifier = Modifier.fillMaxSize()) {
        SettingsTopBar(onBack = onBack)
        SettingsConfigSection(
            config = config,
            syncReady = syncReady,
            onRequestLeave = { confirmLeave = true },
        )
    }

    if (confirmLeave) {
        AlertDialog(
            onDismissRequest = { confirmLeave = false },
            title = { Text(stringResource(R.string.settings_remove_library_confirm_title)) },
            text = { Text(stringResource(R.string.settings_remove_library_confirm_body)) },
            confirmButton = {
                TextButton(onClick = {
                    confirmLeave = false
                    onLeaveLibrary()
                }) {
                    Text(stringResource(R.string.settings_remove))
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmLeave = false }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
}

@Composable
private fun SettingsTopBar(onBack: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
            }
            Text(
                text = stringResource(R.string.settings),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
            )
        }
    }
}

@Composable
private fun SettingsConfigSection(
    config: BridgeConfig,
    syncReady: Boolean,
    onRequestLeave: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = stringResource(R.string.settings_sync),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(stringResource(if (config.sync != null) R.string.settings_cloud_sync_on else R.string.settings_local_only))
        if (config.sync != null) {
            Text(
                text = stringResource(if (syncReady) R.string.settings_synced else R.string.settings_syncing),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(modifier = Modifier.height(24.dp))
        Button(
            onClick = onRequestLeave,
            colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error),
        ) {
            Text(stringResource(R.string.settings_remove_library))
        }
        Text(
            text = stringResource(R.string.settings_remove_library_explanation),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
