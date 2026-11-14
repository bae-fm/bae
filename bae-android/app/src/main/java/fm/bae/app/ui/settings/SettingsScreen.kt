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
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.Logo
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

    /** The library has no storage, which a phone's never lacks: a reported bug, and Settings says so in its place. */
    data object Missing : Storage

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

    when (page) {
        null -> {
            val libraryModel = rememberSettingsLibraryModel(session)
            LaunchedEffect(libraryModel) { libraryModel.loadStorage() }
            val line by session.syncStatusStore.line.collectAsState()
            val sync = syncRowSummary(line)
            val deviceStorage = rememberDeviceStorageModel(session)
            AndroidSettingsList(
                summaries = rememberSettingsSummaries(config, libraryModel.name, sync, deviceStorage.read),
                onBack = onBack,
                onOpen = { page = it },
                onPush = onPush,
            ) {
                SettingsLibrarySwitcher(
                    listing = listing,
                    onReloadLibraries = actions.reloadLibraries,
                    onSwitchLibrary = actions.switchLibrary,
                )
            }
        }

        SettingsPage.Appearance -> {
            AndroidSettingsAppearance(onBack = { page = null })
        }

        SettingsPage.Playback -> {
            SettingsPlaybackSection(
                session = session,
                config = config,
                ioDispatcher = ioDispatcher,
                onBack = { page = null },
            )
        }

        SettingsPage.About -> {
            AndroidSettingsAbout(version = installedVersion(), onBack = { page = null })
        }
    }
}

/** The Settings list: a row for each page, then [below]. */
@Composable
internal fun AndroidSettingsList(
    summaries: SettingsSummaries,
    onBack: () -> Unit,
    onOpen: (SettingsPage) -> Unit,
    onPush: (LibraryDestination) -> Unit,
    below: @Composable () -> Unit = {},
) {
    AndroidScreen(title = stringResource(R.string.settings), onBack = onBack, modifier = Modifier.fillMaxSize()) {
        Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            SettingsMenu(summaries = summaries, onOpen = onOpen, onPush = onPush)
            below()
        }
    }
}

/** The About page: the sheep over the installed [version]. */
@Composable
internal fun AndroidSettingsAbout(
    version: String,
    onBack: () -> Unit,
) {
    AndroidScreen(title = stringResource(R.string.settings_about), onBack = onBack, modifier = Modifier.fillMaxSize()) {
        Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            SettingsAboutSection(version)
        }
    }
}

/** The Appearance page: theme, tone and accent. */
@Composable
internal fun AndroidSettingsAppearance(onBack: () -> Unit) {
    AndroidScreen(
        title = stringResource(R.string.appearance_title),
        onBack = onBack,
        modifier = Modifier.fillMaxSize(),
    ) {
        Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            AppearanceSection()
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
            modifier = Modifier.fillMaxWidth().padding(top = ThemeSpace.page, bottom = ThemeSpace.section),
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
