package fm.bae.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.localizedLine
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibrary

private const val TAG = "bae.SettingsScreen"
private val logger = BaeLogger(TAG)

/**
 * Per-device settings: the library's sync status, device management (the
 * membership chain), an on-demand recovery-code reveal, and a destructive
 * action to remove the library from this device. Reached from the gear in the
 * library top bar.
 */
@Composable
fun SettingsScreen(
    session: OpenLibrary,
    libraries: StateFlow<List<BridgeLibrary>>,
    onBack: () -> Unit,
    onManageDevices: () -> Unit,
    onSwitchLibrary: (BridgeLibrary) -> Unit,
    onLeaveLibrary: () -> Unit,
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    val config by session.configStore.config.collectAsState()
    val syncReady by session.configStore.syncReady.collectAsState()
    val syncError by session.configStore.syncError.collectAsState()
    val allLibraries by libraries.collectAsState()
    var confirmLeave by remember { mutableStateOf(false) }
    var showRecoveryCode by remember { mutableStateOf(false) }

    Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        SettingsTopBar(onBack = onBack)
        if (allLibraries.size > 1) {
            SettingsLibrarySection(
                libraries = allLibraries,
                activeLibraryId = session.libraryId,
                onSwitchLibrary = onSwitchLibrary,
            )
            HorizontalDivider(modifier = Modifier.padding(horizontal = 16.dp))
        }
        SettingsConfigSection(
            session = session,
            config = config,
            syncReady = syncReady,
            syncError = syncError,
            ioDispatcher = ioDispatcher,
        )
        // Managing devices and revealing the recovery code both read the
        // membership chain from the library's cloud storage, so they need a live
        // sync session this run — gate on runtime sync readiness, not merely a
        // configured provider (which can be present while sync is broken).
        if (syncReady) {
            HorizontalDivider(modifier = Modifier.padding(horizontal = 16.dp))
            SettingsDevicesSection(
                onManageDevices = onManageDevices,
                onRevealRecoveryCode = { showRecoveryCode = true },
            )
        }
        HorizontalDivider(modifier = Modifier.padding(horizontal = 16.dp))
        SettingsLeaveSection(onRequestLeave = { confirmLeave = true })
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

    if (showRecoveryCode) {
        RecoveryCodeDialog(session = session, onDismiss = { showRecoveryCode = false })
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
private fun SettingsLibrarySection(
    libraries: List<BridgeLibrary>,
    activeLibraryId: String,
    onSwitchLibrary: (BridgeLibrary) -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = stringResource(R.string.settings_library),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        libraries.forEach { library ->
            // The active library is derived from the open session, not the
            // BridgeLibrary.isActive snapshot: that snapshot is taken at launch
            // discovery and goes stale after an in-app switch.
            val isActive = library.id == activeLibraryId
            Row(
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .clickable(enabled = !isActive) { onSwitchLibrary(library) },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(text = library.name, modifier = Modifier.weight(1f))
                // Always present but alpha-toggled so switching the active row
                // never re-measures the row heights.
                Icon(
                    imageVector = Icons.Filled.Check,
                    contentDescription = null,
                    modifier = Modifier.alpha(if (isActive) 1f else 0f),
                )
            }
        }
    }
}

@Composable
private fun SettingsConfigSection(
    session: OpenLibrary,
    config: BridgeConfig,
    syncReady: Boolean,
    syncError: String?,
    ioDispatcher: CoroutineDispatcher,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
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
        config.sync?.let { sync ->
            SyncConnectedControls(
                session = session,
                sync = sync,
                syncReady = syncReady,
                syncError = syncError,
                ioDispatcher = ioDispatcher,
            )
        }
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = stringResource(R.string.settings_pause_between_sides),
                modifier = Modifier.weight(1f),
            )
            Switch(
                checked = config.pauseBetweenSides,
                onCheckedChange = { enabled ->
                    scope.launch {
                        try {
                            withContext(ioDispatcher) {
                                session.appHandle.setPauseBetweenSides(enabled)
                            }
                        } catch (e: CancellationException) {
                            throw e
                        } catch (e: BridgeException) {
                            logger.error("Failed to update pause-between-sides setting", e)
                            session.configStore.showError(context.localizedLine(e))
                        } catch (e: Exception) {
                            logger.error("Failed to update pause-between-sides setting", e)
                            session.configStore.showError(e.toString())
                        }
                    }
                },
            )
        }
    }
}

@Composable
private fun SettingsDevicesSection(
    onManageDevices: () -> Unit,
    onRevealRecoveryCode: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = stringResource(R.string.settings_devices),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Button(onClick = onManageDevices) {
            Text(stringResource(R.string.settings_manage_devices))
        }
        Spacer(modifier = Modifier.height(8.dp))
        Text(
            text = stringResource(R.string.settings_recovery_code),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            text = stringResource(R.string.settings_recovery_code_explanation),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        OutlinedButton(onClick = onRevealRecoveryCode) {
            Text(stringResource(R.string.settings_reveal_recovery_code))
        }
    }
}

@Composable
private fun SettingsLeaveSection(onRequestLeave: () -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
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

/**
 * Reveals the library's recovery code on demand. The code is a bearer secret —
 * anyone holding it can restore the whole library — so it is generated only when
 * the user asks and labelled as a secret, never offered as an add-a-device step
 * (devices are added through the membership chain in the Devices screen).
 */
@Composable
private fun RecoveryCodeDialog(
    session: OpenLibrary,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var code by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(Unit) {
        try {
            code = withContext(Dispatchers.IO) { session.appHandle.generateRestoreCode() }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to generate recovery code", e)
            error = e.message ?: context.getString(R.string.settings_recovery_code_failed)
        }
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.settings_recovery_code)) },
        text = {
            Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
                Text(
                    text = stringResource(R.string.settings_recovery_code_secret_warning),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
                Spacer(modifier = Modifier.height(12.dp))
                val current = code
                when {
                    current != null -> {
                        Text(
                            text = current,
                            style = MaterialTheme.typography.bodyMedium,
                            fontFamily = FontFamily.Monospace,
                        )
                    }

                    error != null -> {
                        Text(text = error!!, color = MaterialTheme.colorScheme.error)
                    }

                    else -> {
                        CircularProgressIndicator(modifier = Modifier.size(24.dp))
                    }
                }
            }
        },
        confirmButton = {
            val current = code
            if (current != null) {
                TextButton(onClick = { clipboard.setText(AnnotatedString(current)) }) {
                    Icon(Icons.Filled.ContentCopy, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(modifier = Modifier.width(6.dp))
                    Text(stringResource(R.string.settings_copy_recovery_code))
                }
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
        },
    )
}
