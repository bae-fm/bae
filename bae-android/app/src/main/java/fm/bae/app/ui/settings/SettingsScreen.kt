package fm.bae.app.ui.settings

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
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.LibraryListing
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.library.LibraryDestination
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeScreen
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeTelemetryEvent
import uniffi.bae_bridge.BridgeUnsentWork

/** Where the open library is stored, as Settings knows it. */
internal sealed interface CloudHome {
    data object Loading : CloudHome

    /** The library has no cloud home and lives on this device only. */
    data object None : CloudHome

    data class At(
        val location: BridgeStorageLocation,
    ) : CloudHome

    data class Failed(
        val error: DisplayError,
    ) : CloudHome
}

/** Per-device settings, reached from the gear in the library top bar. */
@Composable
internal fun SettingsScreen(
    session: OpenLibrary,
    libraries: StateFlow<LibraryListing>,
    actions: SessionActions,
    onBack: () -> Unit,
    onPush: (LibraryDestination) -> Unit,
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    val scope = rememberScreenScope()
    val config by session.configStore.config.collectAsState()
    val listing by libraries.collectAsState()
    val model = remember(session) { SettingsLibraryModel(session.appHandle, session.errors, scope) }

    LaunchedEffect(session) {
        session.diagnostics.event(BridgeTelemetryEvent.ScreenOpened(BridgeScreen.SETTINGS))
        model.load()
    }
    DisposableEffect(model) {
        model.watch()
        onDispose { model.stopWatching() }
    }
    // A storage setup records the library's location as it ends, and one can end with no setup screen to hear it: one
    // sync finishes after a relaunch, or one whose screen closed past its point of no return.
    val setupRunning by session.syncStatusStore.setupRunning.collectAsState()
    var setupWasRunning by remember { mutableStateOf(setupRunning) }
    LaunchedEffect(setupRunning) {
        if (setupWasRunning && !setupRunning) model.loadCloudHome()
        setupWasRunning = setupRunning
    }

    val self = model.self
    Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        SettingsTopBar(title = stringResource(R.string.settings), onBack = onBack)
        AppearanceSection()
        SectionDivider()
        SettingsLibrarySection(
            session = session,
            listing = listing,
            onReloadLibraries = actions.reloadLibraries,
            name = model.name,
            self = self,
            onRetryName = model::watchName,
            onRetrySelf = { scope.launch { model.loadSelf() } },
            onSwitchLibrary = actions.switchLibrary,
        )
        SectionDivider()
        SettingsSyncSection(
            onResetDevice = actions.resetDevice,
            session = session,
            cloudHome = model.cloudHome,
            oauthProviders = actions.oauthProviders,
            onPush = onPush,
        )
        SelfHistorySharing(
            session = session,
            self = self,
            requested = model.requestedProfile,
            onReloadSelf = { scope.launch { model.loadSelf() } },
        )
        SettingsPlaybackSections(session = session, config = config, ioDispatcher = ioDispatcher, onPush = onPush)
        // People, devices and the recovery code live in the library's store, which only a cloud home has.
        (model.cloudHome as? CloudHome.At)?.let { home ->
            SettingsStoreSections(session = session, home = home, adminChanges = model.adminChanges, onPush = onPush)
        }
        SettingsClosingSections(session = session, onRemove = actions.removeLibrary)
    }
}

/** What lives in the library's store: its people and the changes this phone asked of them, and the recovery code. */
@Composable
private fun SettingsStoreSections(
    session: OpenLibrary,
    home: CloudHome.At,
    adminChanges: List<BridgeAdminChange>,
    onPush: (LibraryDestination) -> Unit,
) {
    SectionDivider()
    SettingsPeopleSection(
        adminChanges = adminChanges,
        onDismissChange = { change ->
            session.messages.perform("dismiss an admin change") {
                session.appHandle.acknowledgeAdminChange(change.operation)
            }
        },
        onManagePeople = { onPush(LibraryDestination.Members) },
    )
    SectionDivider()
    RecoveryCodeSection(
        needsS3Key = home.location is BridgeStorageLocation.S3,
        generate = { session.appHandle.generateRestoreCode().code },
    )
}

/** How this device plays: sides and discs, restoring on launch, and casting. */
@Composable
private fun SettingsPlaybackSections(
    session: OpenLibrary,
    config: BridgeConfig,
    ioDispatcher: CoroutineDispatcher,
    onPush: (LibraryDestination) -> Unit,
) {
    SectionDivider()
    SettingsPlaybackSection(session = session, config = config, ioDispatcher = ioDispatcher)
    SectionDivider()
    SettingsCastingRow(onOpen = { onPush(LibraryDestination.Casting) })
}

/** The row that opens the Casting page. */
@Composable
private fun SettingsCastingRow(onOpen: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().clickable(onClick = onOpen).padding(ThemeSpace.edge),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text = stringResource(R.string.settings_casting), modifier = Modifier.weight(1f))
        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null)
    }
}

/**
 * Removing the library from this device, once confirmed, and what is
 * installed. What removal would lose is read first, while the library keeps
 * running; cancelling leaves it as it was.
 */
@Composable
private fun SettingsClosingSections(
    session: OpenLibrary,
    onRemove: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    var unsent by remember { mutableStateOf<BridgeUnsentWork?>(null) }
    SectionDivider()
    SettingsRemoveSection(onRequestRemove = {
        session.messages.perform("read what removing the library loses") {
            unsent = session.appHandle.unsentBeforeRemoval()
        }
    })
    SectionDivider()
    SettingsAboutSection()
    unsent?.let { work ->
        RemoveLibraryConfirmDialog(
            unsent = work,
            onConfirm = {
                unsent = null
                onRemove()
            },
            onDismiss = { unsent = null },
        )
    }
}

@Composable
internal fun SectionDivider() {
    HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
}

/** The confirmation's text: what removing would lose, that removing loses it, then what removing does. */
@Composable
private fun RemoveLibraryConfirmDialog(
    unsent: BridgeUnsentWork,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val lost =
        listOfNotNull(
            unsent.writes.takeIf { it > 0u }?.let {
                context.coreString("core.library.removal.unsent_writes", mapOf("count" to it.toLong()))
            },
            unsent.files.takeIf { it > 0u }?.let {
                context.coreString("core.library.removal.unsent_files", mapOf("count" to it.toLong()))
            },
        )
    val consequence = if (lost.isEmpty()) emptyList() else listOf(stringResource(R.string.unsent_edits_body))
    val body = (lost + consequence + stringResource(R.string.settings_remove_library_confirm_body)).joinToString(" ")
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.settings_remove_library_confirm_title)) },
        text = { Text(body) },
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

/** The library's people and their devices, where this person also adds their own devices. */
@Composable
private fun SettingsPeopleSection(
    adminChanges: List<BridgeAdminChange>,
    onDismissChange: (BridgeAdminChange) -> Unit,
    onManagePeople: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_people))
        PrimaryButton(onClick = onManagePeople) {
            Text(stringResource(R.string.members_title))
        }
        AdminChangesList(changes = adminChanges, onDismiss = onDismissChange)
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
        SettingsPeopleSection(adminChanges = emptyList(), onDismissChange = {}, onManagePeople = {})
    }
}
