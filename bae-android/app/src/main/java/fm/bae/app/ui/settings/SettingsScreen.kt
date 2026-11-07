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
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.LibraryListing
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.CardRow
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.RowCard
import fm.bae.app.ui.library.LibraryDestination
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeScreen
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeTelemetryEvent

/** Where the open library is stored, as Settings knows it. */
internal sealed interface Storage {
    data object Loading : Storage

    /** The library has no storage and lives on this device only. */
    data object None : Storage

    data class At(
        val location: BridgeStorageLocation,
    ) : Storage

    data class Failed(
        val error: DisplayError,
    ) : Storage
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
        model.loadSelf()
    }
    DisposableEffect(model) {
        model.watchRequestedProfile()
        onDispose { model.stopWatchingRequestedProfile() }
    }

    val self = model.self
    Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        SettingsTopBar(title = stringResource(R.string.settings), onBack = onBack)
        AppearanceSection()
        SectionDivider()
        SettingsLibraryEntry(onOpen = { onPush(LibraryDestination.Library) })
        SettingsLibrarySwitcher(
            session = session,
            listing = listing,
            onReloadLibraries = actions.reloadLibraries,
            onSwitchLibrary = actions.switchLibrary,
        )
        SelfHistorySharing(
            session = session,
            self = self,
            requested = model.requestedProfile,
            onReloadSelf = { scope.launch { model.loadSelf() } },
        )
        SettingsPlaybackSections(session = session, config = config, ioDispatcher = ioDispatcher, onPush = onPush)
        SectionDivider()
        SettingsTransfersEntry(onOpen = { onPush(LibraryDestination.Transfers) })
        SectionDivider()
        SettingsAboutSection()
    }
}

/** The row that opens the Library page. */
@Composable
private fun SettingsLibraryEntry(onOpen: () -> Unit) {
    Column(modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge)) {
        RowCard {
            CardRow(title = stringResource(R.string.settings_library), onClick = onOpen)
        }
    }
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

@Composable
internal fun SectionDivider() {
    HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
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
