package fm.bae.app.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.SyncFailure
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.PrimaryButton
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.StateFlow
import uniffi.bae_bridge.BridgeBlockedSyncOperation
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeScreen
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeTelemetryEvent

/** Per-device settings, reached from the gear in the library top bar. */
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
    val syncSnapshot by session.syncStatusStore.snapshot.collectAsState()
    val syncIndicator by session.syncStatusStore.indicator.collectAsState()
    val syncError by session.syncStatusStore.error.collectAsState()
    val syncBlocked by session.syncStatusStore.blocked.collectAsState()
    val allLibraries by libraries.collectAsState()
    var confirmLeave by remember { mutableStateOf(false) }
    var showRecoveryCode by remember { mutableStateOf(false) }

    ReportScreenOpened(session, BridgeScreen.SETTINGS)

    SettingsSections(
        session = session,
        config = config,
        libraries = allLibraries,
        syncIndicator = syncIndicator,
        syncError = syncError,
        syncBlocked = syncBlocked,
        syncReady = syncSnapshot?.syncReady == true,
        ioDispatcher = ioDispatcher,
        onBack = onBack,
        onSwitchLibrary = onSwitchLibrary,
        onManageDevices = onManageDevices,
        onRevealRecoveryCode = { showRecoveryCode = true },
        onRequestLeave = { confirmLeave = true },
    )

    if (confirmLeave) {
        LeaveLibraryConfirmDialog(
            onConfirm = {
                confirmLeave = false
                onLeaveLibrary()
            },
            onDismiss = { confirmLeave = false },
        )
    }

    if (showRecoveryCode) {
        RecoveryCodeDialog(session = session, onDismiss = { showRecoveryCode = false })
    }
}

/** The settings screen's sections, top to bottom, in one scrolling column. */
@Composable
private fun SettingsSections(
    session: OpenLibrary,
    config: BridgeConfig,
    libraries: List<BridgeLibrary>,
    syncIndicator: BridgeSyncIndicator,
    syncError: SyncFailure?,
    syncBlocked: List<BridgeBlockedSyncOperation>,
    syncReady: Boolean,
    ioDispatcher: CoroutineDispatcher,
    onBack: () -> Unit,
    onSwitchLibrary: (BridgeLibrary) -> Unit,
    onManageDevices: () -> Unit,
    onRevealRecoveryCode: () -> Unit,
    onRequestLeave: () -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        SettingsTopBar(onBack = onBack)
        AppearanceSection()
        HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
        if (libraries.size > 1) {
            SettingsLibrarySection(
                libraries = libraries,
                activeLibraryId = session.libraryId,
                onSwitchLibrary = onSwitchLibrary,
            )
            HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
        }
        SettingsConfigSection(
            session = session,
            config = config,
            syncIndicator = syncIndicator,
            syncError = syncError,
            syncBlocked = syncBlocked,
        )
        HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
        SettingsPlaybackSection(
            session = session,
            config = config,
            ioDispatcher = ioDispatcher,
        )
        HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
        SettingsCastSection(
            session = session,
            config = config,
            ioDispatcher = ioDispatcher,
        )
        // Devices and the recovery code read the membership chain from cloud storage, so they need sync running now.
        if (syncReady) {
            HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
            SettingsDevicesSection(
                onManageDevices = onManageDevices,
                onRevealRecoveryCode = onRevealRecoveryCode,
            )
        }
        HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
        SettingsLeaveSection(onRequestLeave = onRequestLeave)
        HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
        SettingsAboutSection()
    }
}

/** Reports the screen open as a telemetry event when this enters the composition. */
@Composable
private fun ReportScreenOpened(
    session: OpenLibrary,
    screen: BridgeScreen,
) {
    LaunchedEffect(Unit) {
        session.diagnostics.event(BridgeTelemetryEvent.ScreenOpened(screen))
    }
}

@Composable
private fun LeaveLibraryConfirmDialog(
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.settings_remove_library_confirm_title)) },
        text = { Text(stringResource(R.string.settings_remove_library_confirm_body)) },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text(stringResource(R.string.settings_remove))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

@Composable
private fun SettingsTopBar(onBack: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
            }
            Text(
                text = stringResource(R.string.settings),
                style = ThemeText.title.style,
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
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_library))
        libraries.forEach { library ->
            // The active library comes from the open session; BridgeLibrary.isActive goes stale after an in-app switch.
            val isActive = library.id == activeLibraryId
            val error = library.error
            Row(
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .clickable(enabled = !isActive && error == null) { onSwitchLibrary(library) },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(text = library.name)
                    // A library whose config won't load stays listed.
                    if (error != null) {
                        ErrorText(error, maxLines = 2)
                    }
                }
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
    syncIndicator: BridgeSyncIndicator,
    syncError: SyncFailure?,
    syncBlocked: List<BridgeBlockedSyncOperation>,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_sync))
        Text(stringResource(if (config.sync != null) R.string.settings_cloud_sync_on else R.string.settings_local_only))
        config.sync?.let { sync ->
            SyncConnectedControls(
                session = session,
                sync = sync,
                indicator = syncIndicator,
                syncError = syncError,
                blocked = syncBlocked,
            )
        }
    }
}

@Composable
private fun SettingsAboutSection() {
    val context = LocalContext.current
    // Read from the package rather than BuildConfig so it reflects what is installed.
    val versionName =
        remember(context) {
            checkNotNull(context.packageManager.getPackageInfo(context.packageName, 0).versionName) {
                "the app manifest always carries versionName"
            }
        }
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_about))
        Row(modifier = Modifier.fillMaxWidth()) {
            Text(
                text = stringResource(R.string.settings_version),
                modifier = Modifier.weight(1f),
            )
            Text(
                text = versionName,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
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
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_devices))
        PrimaryButton(onClick = onManageDevices) {
            Text(stringResource(R.string.settings_manage_devices))
        }
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        Eyebrow(text = stringResource(R.string.settings_recovery_code))
        Text(
            text = stringResource(R.string.settings_recovery_code_explanation),
            style = ThemeText.detail.style,
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
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        PrimaryButton(
            onClick = onRequestLeave,
            colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error),
        ) {
            Text(stringResource(R.string.settings_remove_library))
        }
        Text(
            text = stringResource(R.string.settings_remove_library_explanation),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun SettingsLibrarySectionPreview() {
    BaeTheme {
        SettingsLibrarySection(
            libraries =
                listOf(
                    PreviewData.library(),
                    PreviewData.library(id = "lib-2", name = "Other Library", isActive = false),
                ),
            activeLibraryId = "lib-1",
            onSwitchLibrary = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun SettingsDevicesSectionPreview() {
    BaeTheme {
        SettingsDevicesSection(onManageDevices = {}, onRevealRecoveryCode = {})
    }
}
