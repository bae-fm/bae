package fm.bae.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
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
import androidx.compose.ui.unit.dp
import fm.bae.app.AppScreen
import fm.bae.app.AppSessionHolder
import fm.bae.app.OAuthLinker
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ShortcutAction
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.library.LibraryScreen
import fm.bae.app.ui.onboarding.OnboardingScreen
import fm.bae.app.ui.onboarding.UnlockScreen
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeLibrary

/**
 * App root. Drives the [AppScreen] lifecycle: discover an existing library and
 * open it, or onboard. After onboarding restores a library it opens it; an
 * unlocked library shows the real library UI.
 */
@Composable
fun ContentView(
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    shortcutAction: ShortcutAction?,
    onShortcutHandled: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var screen by remember { mutableStateOf<AppScreen>(AppScreen.Loading) }

    // On launch, go straight to an already-open session (e.g. after the
    // composition is recreated), else open the first discovered library or fall
    // through to onboarding.
    LaunchedEffect(Unit) {
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

/**
 * The app's root chrome: theme, the full-size background surface, and the
 * safe-drawing inset padding every screen sits inside. Screen bodies render
 * inside this. Screenshot captures render a scene inside the same chrome so a
 * captured screen looks exactly like the shipped one.
 */
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
 * The unlocked library UI plus the app-level snackbar host. Play Next / Add to
 * Queue confirm here with a transient "+N added" snackbar, surfaced wherever the
 * action was triggered — the now-playing bar that carries the queue button is
 * hidden while nothing plays, so a badge there would give no feedback when
 * building a queue from an idle state. collectLatest replaces a still-showing
 * confirmation with the newer count rather than queuing them.
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
    // The Resume shortcut opens the app straight into playback. Opening the
    // library already restored the queue paused at the saved position, so this
    // is the explicit resume that turns that restore into audio. The resulting
    // retained playing value brings the foreground playback service up (the app
    // is foregrounded by the launch), the same path an on-screen play uses. Keyed on
    // the session + action so it fires once and re-arms only for a fresh request;
    // the Search shortcut seeds the browser through LibraryScreen instead.
    LaunchedEffect(session, shortcutAction) {
        if (shortcutAction == ShortcutAction.RESUME) {
            session.appHandle.resume()
            onShortcutHandled()
        }
    }
    // The image store is the open library's: its cache entries are keyed on that
    // library's image ids, so it is scoped to the session, not the process.
    CompositionLocalProvider(LocalImageStore provides session.imageStore) {
        Box(modifier = Modifier.fillMaxSize()) {
            LibraryScreen(
                session = session,
                libraries = libraries,
                openSearch = shortcutAction == ShortcutAction.SEARCH,
                onSearchOpened = onShortcutHandled,
                onSwitchLibrary = onSwitchLibrary,
                onLeaveLibrary = onLeaveLibrary,
            )
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
        modifier = Modifier.fillMaxSize().padding(32.dp),
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
