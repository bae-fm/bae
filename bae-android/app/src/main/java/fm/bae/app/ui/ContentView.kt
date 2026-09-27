package fm.bae.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.AppScreen
import fm.bae.app.AppSessionHolder
import fm.bae.app.OAuthLinker
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ShortcutAction
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.library.ArtworkLoadingBanner
import fm.bae.app.ui.library.LibraryScreen
import fm.bae.app.ui.onboarding.OnboardingScreen
import fm.bae.app.ui.onboarding.UnlockScreen
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeLibrary

/** App root: opens an existing library or onboards, then shows the library once unlocked. */
@Composable
fun ContentView(
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    startupError: String?,
    shortcutAction: ShortcutAction?,
    onShortcutHandled: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var screen by remember(startupError) {
        mutableStateOf<AppScreen>(
            startupError?.let(AppScreen::Failed) ?: AppScreen.Loading,
        )
    }

    // Reuse a session that is already open, else open the first discovered library or onboard.
    LaunchedEffect(startupError) {
        if (startupError != null) return@LaunchedEffect
        val open = AppSessionHolder.currentSession()
        if (open != null) {
            screen = AppScreen.LibraryOpen(open)
            return@LaunchedEffect
        }
        AppSessionHolder.openDiscoveredOrOnboard(context) { screen = it }
    }

    BaeAppChrome {
        AppScreenRouter(
            screen = screen,
            oauthLinking = oauthLinking,
            oauthLinkingError = oauthLinkingError,
            shortcutAction = shortcutAction,
            onShortcutHandled = onShortcutHandled,
            onScreen = { screen = it },
        )
    }
}

/** Theme, background and safe-drawing inset every screen sits in; screenshot captures use it too. */
@Composable
internal fun BaeAppChrome(content: @Composable () -> Unit) {
    BaeTheme {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.background,
        ) {
            Box(modifier = Modifier.fillMaxSize().safeDrawingPadding()) {
                content()
            }
        }
    }
}

@Composable
private fun AppScreenRouter(
    screen: AppScreen,
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    shortcutAction: ShortcutAction?,
    onShortcutHandled: () -> Unit,
    onScreen: (AppScreen) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    when (val current = screen) {
        AppScreen.Loading -> {
            LoadingScreen()
        }

        AppScreen.Onboarding -> {
            OnboardingScreen(
                oauthLinking = oauthLinking,
                oauthLinkingError = oauthLinkingError,
                onLinked = { info ->
                    AppSessionHolder.onLinked(info)
                    scope.launch { AppSessionHolder.openLibrary(context, info.id, onScreen) }
                },
            )
        }

        is AppScreen.Unlock -> {
            UnlockScreen(
                libraryName = current.libraryName,
                onUnlock = { key ->
                    val session = AppSessionHolder.unlock(key)
                    onScreen(AppScreen.LibraryOpen(session))
                },
                onCancel = {
                    AppSessionHolder.cancelUnlock()
                    val open = AppSessionHolder.currentSession()
                    onScreen(if (open != null) AppScreen.LibraryOpen(open) else AppScreen.Onboarding)
                },
            )
        }

        is AppScreen.LibraryOpen -> {
            LibraryOpenScreen(
                session = current.session,
                libraries = AppSessionHolder.libraries,
                shortcutAction = shortcutAction,
                onShortcutHandled = onShortcutHandled,
                onSwitchLibrary = { library ->
                    scope.launch { AppSessionHolder.openLibrary(context, library.id, onScreen) }
                },
                onLeaveLibrary = {
                    scope.launch { AppSessionHolder.forgetActiveLibrary(context, onScreen) }
                },
            )
        }

        is AppScreen.Failed -> {
            FailedScreen(message = current.message)
        }
    }
}

/**
 * The unlocked library UI plus the snackbar that confirms Play Next and Add to
 * Queue, since the now-playing bar is hidden while nothing plays.
 */
@Composable
private fun LibraryOpenScreen(
    session: OpenLibrary,
    libraries: StateFlow<List<BridgeLibrary>>,
    shortcutAction: ShortcutAction?,
    onShortcutHandled: () -> Unit,
    onSwitchLibrary: (BridgeLibrary) -> Unit,
    onLeaveLibrary: () -> Unit,
) {
    val snackbarHostState = remember { SnackbarHostState() }
    val appContext = LocalContext.current
    LaunchedEffect(session.playback, snackbarHostState) {
        session.playback.queueItemsAdded.collectLatest { count ->
            snackbarHostState.showSnackbar(
                appContext.resources.getQuantityString(R.plurals.queue_items_added, count, count),
            )
        }
    }
    // The Resume shortcut plays the queue that opening the library restored paused.
    LaunchedEffect(session, shortcutAction) {
        if (shortcutAction == ShortcutAction.RESUME) {
            session.appHandle.resume()
            onShortcutHandled()
        }
    }
    // Scoped to the session because the cache is keyed on that library's image ids.
    CompositionLocalProvider(LocalImageStore provides session.imageStore) {
        Column(modifier = Modifier.fillMaxSize()) {
            ArtworkLoadingBanner(session.artworkLoadingStore)
            Box(modifier = Modifier.fillMaxWidth().weight(1f)) {
                LibraryScreen(
                    session = session,
                    libraries = libraries,
                    openSearch = shortcutAction == ShortcutAction.SEARCH,
                    onSearchOpened = onShortcutHandled,
                    onSwitchLibrary = onSwitchLibrary,
                    onLeaveLibrary = onLeaveLibrary,
                )
            }
        }
        Box(modifier = Modifier.fillMaxSize()) {
            SnackbarHost(
                hostState = snackbarHostState,
                modifier = Modifier.align(Alignment.BottomCenter),
            )
        }
    }
}

@Composable
private fun LoadingScreen() {
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        CircularProgressIndicator()
    }
}

@Composable
private fun FailedScreen(message: String) {
    Box(
        modifier = Modifier.fillMaxSize().padding(ThemeSpace.page),
        contentAlignment = Alignment.Center,
    ) {
        Text(text = message, color = MaterialTheme.colorScheme.error)
    }
}

@Preview(showBackground = true)
@Composable
private fun LoadingScreenPreview() {
    BaeTheme {
        LoadingScreen()
    }
}

@Preview(showBackground = true)
@Composable
private fun FailedScreenPreview() {
    BaeTheme {
        FailedScreen(message = "Couldn't open the library.")
    }
}
