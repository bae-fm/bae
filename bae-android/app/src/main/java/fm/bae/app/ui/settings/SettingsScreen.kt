package fm.bae.app.ui.settings

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.LibraryListing
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.Logo
import fm.bae.app.ui.components.ScreenTopBar
import fm.bae.app.ui.library.LibraryDestination
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.StateFlow
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
    val config by session.configStore.config.collectAsState()
    val listing by libraries.collectAsState()

    LaunchedEffect(session) {
        session.diagnostics.event(BridgeTelemetryEvent.ScreenOpened(BridgeScreen.SETTINGS))
    }

    var page by rememberSaveable { mutableStateOf<SettingsPage?>(null) }
    BackHandler(enabled = page != null) { page = null }

    val open = page
    Column(modifier = Modifier.fillMaxSize()) {
        ScreenTopBar(
            title = stringResource(open?.title ?: R.string.settings),
            onBack = if (open == null) onBack else ({ page = null }),
        )
        Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            when (open) {
                null -> {
                    val libraryModel = rememberSettingsLibraryModel(session)
                    LaunchedEffect(libraryModel) { libraryModel.loadStorage() }
                    val line by session.syncStatusStore.line.collectAsState()
                    val setupRunning by session.syncStatusStore.setupRunning.collectAsState()
                    val sync = syncRowSummary(libraryModel.storage, line, setupRunning)
                    SettingsMenu(
                        summaries = rememberSettingsSummaries(config, libraryModel.name, sync),
                        onOpen = { page = it },
                        onPush = onPush,
                    )
                    SettingsLibrarySwitcher(
                        session = session,
                        listing = listing,
                        onReloadLibraries = actions.reloadLibraries,
                        onSwitchLibrary = actions.switchLibrary,
                    )
                }

                SettingsPage.Appearance -> {
                    AppearanceSection()
                }

                SettingsPage.Playback -> {
                    SettingsPlaybackSection(session = session, config = config, ioDispatcher = ioDispatcher)
                }

                SettingsPage.About -> {
                    SettingsAboutSection(version = installedVersion())
                }
            }
        }
    }
}

@Composable
internal fun SectionDivider() {
    HorizontalDivider(modifier = Modifier.padding(horizontal = ThemeSpace.edge))
}

@Composable
internal fun SettingsAboutSection(version: String) {
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge)) {
        Box(
            modifier = Modifier.fillMaxWidth().padding(top = ThemeSpace.page, bottom = ThemeSpace.edge),
            contentAlignment = Alignment.Center,
        ) {
            Logo(size = ABOUT_LOGO_SIZE, bob = false, glow = false)
        }
        AndroidCard {
            AndroidRow(title = stringResource(R.string.settings_version), subtitle = version)
        }
    }
}

private val ABOUT_LOGO_SIZE = 96.dp
