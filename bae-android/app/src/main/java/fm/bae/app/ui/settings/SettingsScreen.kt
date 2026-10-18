package fm.bae.app.ui.settings

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.localizedLine
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.library.LibraryDestination
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgeScreen
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeTelemetryEvent

private val logger = BaeLogger("bae.SettingsScreen")

/** Where the open library is stored, as Settings knows it. */
internal sealed interface CloudHome {
    data object Loading : CloudHome

    /** The library has no cloud home and lives on this device only. */
    data object None : CloudHome

    data class At(
        val location: BridgeStorageLocation,
    ) : CloudHome

    data class Failed(
        val message: String,
    ) : CloudHome
}

/**
 * What Settings reads from the open library beyond its config: its cloud home,
 * and this device's person — their profile and whether they are an admin.
 */
private class SettingsLibraryModel(
    private val session: OpenLibrary,
    private val appContext: Context,
) {
    var cloudHome by mutableStateOf<CloudHome>(CloudHome.Loading)
        private set

    /** This device's person; null while unread, and when the library has no members yet. */
    var self by mutableStateOf<BridgePerson?>(null)
        private set

    suspend fun load() {
        cloudHome =
            try {
                session.appHandle.storageLocation()?.let(CloudHome::At) ?: CloudHome.None
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("reading the library's cloud home failed", e)
                CloudHome.Failed(appContext.localizedLine(e) ?: e.toString())
            }
        loadSelf()
    }

    suspend fun loadSelf() {
        self =
            try {
                session.appHandle.people().firstOrNull { it.membership?.isSelf == true }
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("reading the library's people failed", e)
                session.configStore.showError(appContext.localizedLine(e))
                null
            }
    }
}

/** Per-device settings, reached from the gear in the library top bar. */
@Composable
internal fun SettingsScreen(
    session: OpenLibrary,
    libraries: StateFlow<List<BridgeLibrary>>,
    actions: SessionActions,
    onBack: () -> Unit,
    onPush: (LibraryDestination) -> Unit,
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    val appContext = LocalContext.current.applicationContext
    val scope = rememberCoroutineScope()
    val config by session.configStore.config.collectAsState()
    val allLibraries by libraries.collectAsState()
    val model = remember(session) { SettingsLibraryModel(session, appContext) }
    var showRestoreCode by remember { mutableStateOf(false) }

    LaunchedEffect(session) {
        session.diagnostics.event(BridgeTelemetryEvent.ScreenOpened(BridgeScreen.SETTINGS))
        model.load()
    }

    val self = model.self
    Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        SettingsTopBar(onBack = onBack)
        AppearanceSection()
        SectionDivider()
        SettingsLibrarySection(
            session = session,
            libraries = allLibraries,
            canRename = self?.membership?.role == BridgeMemberRole.ADMIN,
            onSwitchLibrary = actions.switchLibrary,
        )
        SectionDivider()
        SettingsSyncSection(
            onResetDevice = actions.resetDevice,
            session = session,
            cloudHome = model.cloudHome,
            onPush = onPush,
            onCloudHomeChanged = { scope.launch { model.load() } },
        )
        self?.profile?.let { profile ->
            SectionDivider()
            HistorySharingSection(
                session = session,
                sharing = profile.historySharing,
                onChanged = { scope.launch { model.loadSelf() } },
            )
        }
        SettingsPlaybackSections(session = session, config = config, ioDispatcher = ioDispatcher)
        // People, devices and restore codes live in the library's store, which only a cloud home has.
        if (model.cloudHome is CloudHome.At) {
            SectionDivider()
            SettingsPeopleSection(
                onManagePeople = { onPush(LibraryDestination.Members) },
                onShowRestoreCode = { showRestoreCode = true },
            )
        }
        SettingsClosingSections(onRemove = actions.removeLibrary)
    }

    if (showRestoreCode) {
        RestoreCodeDialog(session = session, onDismiss = { showRestoreCode = false })
    }
}

/** How this device plays: sides and discs, restoring on launch, and casting. */
@Composable
private fun SettingsPlaybackSections(
    session: OpenLibrary,
    config: BridgeConfig,
    ioDispatcher: CoroutineDispatcher,
) {
    SectionDivider()
    SettingsPlaybackSection(session = session, config = config, ioDispatcher = ioDispatcher)
    SectionDivider()
    SettingsCastSection(session = session, config = config, ioDispatcher = ioDispatcher)
}

/** Removing the library from this device, once confirmed, and what is installed. */
@Composable
private fun SettingsClosingSections(onRemove: () -> Unit) {
    var confirmRemove by remember { mutableStateOf(false) }
    SectionDivider()
    SettingsRemoveSection(onRequestRemove = { confirmRemove = true })
    SectionDivider()
    SettingsAboutSection()
    if (confirmRemove) {
        RemoveLibraryConfirmDialog(
            onConfirm = {
                confirmRemove = false
                onRemove()
            },
            onDismiss = { confirmRemove = false },
        )
    }
}

@Composable
private fun SectionDivider() {
    HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
}

@Composable
private fun RemoveLibraryConfirmDialog(
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

/** The library's people and their devices, and this person's restore code for a new device. */
@Composable
private fun SettingsPeopleSection(
    onManagePeople: () -> Unit,
    onShowRestoreCode: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_people))
        PrimaryButton(onClick = onManagePeople) {
            Text(stringResource(R.string.members_title))
        }
        Eyebrow(text = stringResource(R.string.settings_restore_code))
        Text(
            text = stringResource(R.string.settings_restore_code_explanation),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        OutlinedButton(onClick = onShowRestoreCode) {
            Text(stringResource(R.string.settings_show_restore_code))
        }
    }
}

@Composable
private fun SettingsRemoveSection(onRequestRemove: () -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        PrimaryButton(
            onClick = onRequestRemove,
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
private fun SettingsPeopleSectionPreview() {
    BaeTheme {
        SettingsPeopleSection(onManagePeople = {}, onShowRestoreCode = {})
    }
}
