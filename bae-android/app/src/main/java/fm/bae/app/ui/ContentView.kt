package fm.bae.app.ui

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.AppScreen
import fm.bae.app.AppSessionHolder
import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.FailedStep
import fm.bae.app.LibraryListing
import fm.bae.app.LocalBugReports
import fm.bae.app.OAuthProviders
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ScreenFailure
import fm.bae.app.ShortcutAction
import fm.bae.app.coreString
import fm.bae.app.data.LocalImageStore
import fm.bae.app.data.SessionMessage
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ScreenFailureNotice
import fm.bae.app.ui.library.LibraryScreen
import fm.bae.app.ui.onboarding.OnboardingScreen
import fm.bae.app.ui.onboarding.RestoreOver
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeReplacedBy
import uniffi.bae_bridge.bridgeErrorCategoryKey

/** App root: opens an existing library or onboards, then shows the library once open. */
@Composable
fun ContentView(
    oauthProviders: OAuthProviders?,
    oauthProvidersError: String?,
    startupFailure: ScreenFailure?,
    bugs: BugReports,
    shortcutAction: ShortcutAction?,
    onShortcutHandled: () -> Unit,
) {
    val context = LocalContext.current
    var screen by remember(startupFailure) {
        mutableStateOf<AppScreen>(
            startupFailure?.let(AppScreen::Failed) ?: AppScreen.Loading,
        )
    }

    // Reuse a session that is already open, else open the first discovered library or onboard.
    LaunchedEffect(startupFailure) {
        if (startupFailure != null) return@LaunchedEffect
        val open = AppSessionHolder.currentSession()
        if (open != null) {
            screen = AppScreen.LibraryOpen(open)
            return@LaunchedEffect
        }
        AppSessionHolder.openDiscoveredOrOnboard(context) { screen = it }
    }

    // One snackbar for the app, over every screen: an open library's messages
    // and, on any screen, a bug reported without crashing.
    val snackbarHostState = remember { SnackbarHostState() }
    val somethingWentWrong = context.coreString("core.error.category.internal")
    LaunchedEffect(bugs, snackbarHostState) {
        bugs.reported.collect {
            snackbarHostState.showSnackbar(
                message = somethingWentWrong,
                withDismissAction = true,
                duration = SnackbarDuration.Long,
            )
        }
    }
    BaeAppChrome {
        CompositionLocalProvider(LocalBugReports provides bugs) {
            AppScreenRouter(
                screen = screen,
                oauthProviders = oauthProviders,
                oauthProvidersError = oauthProvidersError,
                shortcutAction = shortcutAction,
                onShortcutHandled = onShortcutHandled,
                snackbarHostState = snackbarHostState,
                onScreen = { screen = it },
            )
        }
        Box(modifier = Modifier.fillMaxSize()) {
            SnackbarHost(
                hostState = snackbarHostState,
                modifier = Modifier.align(Alignment.BottomCenter),
            )
        }
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
    oauthProviders: OAuthProviders?,
    oauthProvidersError: String?,
    shortcutAction: ShortcutAction?,
    onShortcutHandled: () -> Unit,
    snackbarHostState: SnackbarHostState,
    onScreen: (AppScreen) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    when (val current = screen) {
        AppScreen.Loading -> {
            LoadingScreen()
        }

        AppScreen.Onboarding -> {
            OnboardingScreen(
                oauthProvidersError = oauthProvidersError,
                onBootstrapped = { result ->
                    scope.launch { AppSessionHolder.openBootstrapped(context, result, onScreen) }
                },
                restoreOver = null,
            )
        }

        is AppScreen.LibraryOpen -> {
            SessionScreen(current.session) {
                LibraryOpenScreen(
                    session = current.session,
                    libraries = AppSessionHolder.libraries,
                    shortcutAction = shortcutAction,
                    onShortcutHandled = onShortcutHandled,
                    snackbarHostState = snackbarHostState,
                    actions = sessionActions(context, scope, current.session, oauthProviders, onScreen),
                )
            }
        }

        is AppScreen.Recovery -> {
            RecoveryScreen(current, oauthProvidersError, onScreen)
        }

        is AppScreen.ProfileRequired -> {
            SessionScreen(current.session) {
                ProfileRequiredScreen(
                    session = current.session,
                    failure = current.error,
                    onCreated = { onScreen(AppScreen.LibraryOpen(current.session)) },
                )
            }
        }

        is AppScreen.RemovalIncomplete -> {
            RemovalIncompleteScreen(
                error = current.error,
                onRetry = { scope.launch { AppSessionHolder.retryRemoval(context, onScreen) } },
            )
        }

        is AppScreen.Failed -> {
            FailedScreen(failure = current.failure)
        }
    }
}

/** The screen that recovers a library that didn't open or can't sync, with its one call. */
@Composable
private fun RecoveryScreen(
    screen: AppScreen.Recovery,
    oauthProvidersError: String?,
    onScreen: (AppScreen) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val libraryId = screen.libraryId

    // A failed reset shows [failed], the screen it started from, with why.
    fun reset(
        prepared: BridgeDeviceReset,
        failed: (ScreenFailure) -> AppScreen,
    ) {
        scope.launch { AppSessionHolder.resetDevice(context, libraryId, prepared, onScreen, failed) }
    }
    when (val current = screen) {
        is AppScreen.ResetOffered -> {
            ResetOfferedScreen(
                libraryId = libraryId,
                error = current.error,
                onReset = { prepared -> reset(prepared) { AppScreen.ResetOffered(libraryId, it) } },
            )
        }

        is AppScreen.RestoreOffered -> {
            RestoreOverScreen(
                RestoreOver(
                    libraryId = libraryId,
                    title = stringResource(R.string.restore_device_title),
                    message = context.coreString("core.sync.restore_device.warning"),
                    // Going back opens the library again as it was.
                    onBack = { scope.launch { AppSessionHolder.openLibrary(context, libraryId, onScreen) } },
                ),
                oauthProvidersError,
                onScreen,
            )
        }

        is AppScreen.ResetUnfinished -> {
            ResetUnfinishedScreen(
                error = current.error,
                onFinish = {
                    reset(AppSessionHolder.prepareDeviceReset(context, libraryId)) {
                        AppScreen.ResetUnfinished(libraryId, it)
                    }
                },
                onLeave = { AppSessionHolder.leaveFinishing(onScreen) },
            )
        }

        is AppScreen.RestoreUnfinished -> {
            RestoreOverScreen(
                RestoreOver(
                    libraryId = libraryId,
                    title = stringResource(R.string.finish_restore_title),
                    message =
                        context.coreString(
                            bridgeErrorCategoryKey(BridgeErrorCategory.ReplacementUnfinished(BridgeReplacedBy.RESTORE)),
                        ),
                    // The library opens only once the restore finishes, so
                    // going back leaves it waiting.
                    onBack = { AppSessionHolder.leaveFinishing(onScreen) },
                ),
                oauthProvidersError,
                onScreen,
            )
        }
    }
}

/**
 * The person's current restore code restores a library over this device's
 * copy, or finishes such a restore that stopped partway, as [restoreOver]
 * says; the restored library opens.
 */
@Composable
private fun RestoreOverScreen(
    restoreOver: RestoreOver,
    oauthProvidersError: String?,
    onScreen: (AppScreen) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    OnboardingScreen(
        oauthProvidersError = oauthProvidersError,
        onBootstrapped = { result ->
            scope.launch { AppSessionHolder.openBootstrapped(context, result, onScreen) }
        },
        restoreOver = restoreOver,
    )
}

/** What the open library's screens ask of the app, each running in [scope] and ending in a screen. */
private fun sessionActions(
    context: Context,
    scope: CoroutineScope,
    session: OpenLibrary,
    oauthProviders: OAuthProviders?,
    onScreen: (AppScreen) -> Unit,
) = SessionActions(
    oauthProviders = oauthProviders,
    switchLibrary = { library ->
        when (val state = library.state) {
            is BridgeLibraryState.ReplacementUnfinished -> {
                onScreen(AppScreen.finishing(library.id, state.by))
            }

            is BridgeLibraryState.Openable, is BridgeLibraryState.Unreadable -> {
                scope.launch { AppSessionHolder.openLibrary(context, library.id, onScreen) }
            }
        }
    },
    removeLibrary = {
        scope.launch { AppSessionHolder.removeActiveLibrary(context, onScreen) }
    },
    resetDevice = { reset ->
        scope.launch {
            AppSessionHolder.resetDevice(context, session.libraryId, reset, onScreen) { failure ->
                AppScreen.ResetOffered(session.libraryId, failure)
            }
        }
    },
    restoreFromCode = { scope.launch { AppSessionHolder.presentDeviceRestore(onScreen) } },
    reloadLibraries = { scope.launch { AppSessionHolder.reloadLibraries(context) } },
)

/**
 * The open library UI and what the app's snackbar says over its screens: it
 * confirms Play Next and Add to Queue, since the now-playing bar is hidden
 * while nothing plays, and tells the person what failed — what they did, with
 * Retry, and what core reported with no screen of its own, to dismiss.
 */
@Composable
private fun LibraryOpenScreen(
    session: OpenLibrary,
    libraries: StateFlow<LibraryListing>,
    shortcutAction: ShortcutAction?,
    onShortcutHandled: () -> Unit,
    snackbarHostState: SnackbarHostState,
    actions: SessionActions,
) {
    val appContext = LocalContext.current
    LaunchedEffect(session.playback, snackbarHostState) {
        session.playback.queueItemsAdded.collectLatest { count ->
            snackbarHostState.showSnackbar(
                appContext.resources.getQuantityString(R.plurals.queue_items_added, count, count),
            )
        }
    }
    val retryLabel = stringResource(R.string.retry)
    LaunchedEffect(session.messages, snackbarHostState) {
        session.messages.messages.collect { message ->
            showMessage(snackbarHostState, message, retryLabel)
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
        LibraryScreen(
            session = session,
            libraries = libraries,
            openSearch = shortcutAction == ShortcutAction.SEARCH,
            onSearchOpened = onShortcutHandled,
            actions = actions,
        )
    }
}

/** [message] as a snackbar: its Retry runs the failed action again; otherwise it only dismisses. */
private suspend fun showMessage(
    host: SnackbarHostState,
    message: SessionMessage,
    retryLabel: String,
) {
    val result =
        host.showSnackbar(
            message = message.error.line,
            actionLabel = message.retry?.let { retryLabel },
            withDismissAction = true,
            duration = SnackbarDuration.Long,
        )
    if (result == SnackbarResult.ActionPerformed) {
        message.retry?.invoke()
    }
}

@Composable
private fun LoadingScreen() {
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        CircularProgressIndicator()
    }
}

@Composable
private fun FailedScreen(failure: ScreenFailure) {
    Box(
        modifier = Modifier.fillMaxSize().padding(ThemeSpace.page),
        contentAlignment = Alignment.Center,
    ) {
        ScreenFailureNotice(failure)
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
        FailedScreen(
            failure =
                ScreenFailure(
                    FailedStep.LIBRARY_OPEN,
                    DisplayError("Couldn't open the library.", "database error: disk I/O error"),
                ),
        )
    }
}
