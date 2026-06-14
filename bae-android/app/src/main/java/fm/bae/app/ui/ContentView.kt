package fm.bae.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import fm.bae.app.AppScreen
import fm.bae.app.AppSessionHolder
import fm.bae.app.OAuthLinking
import kotlinx.coroutines.launch

/**
 * App root. Drives the [AppScreen] lifecycle: discover an existing library and
 * open it, or onboard. After onboarding restores a library it opens it; an
 * unlocked library shows the real library UI.
 */
@Composable
fun ContentView(
    oauthLinking: OAuthLinking?,
    oauthLinkingError: String?,
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
        val existing = AppSessionHolder.discoverFirstLibrary()
        if (existing == null) {
            screen = AppScreen.Onboarding
        } else {
            AppSessionHolder.openLibrary(context, existing.id) { screen = it }
        }
    }

    BaeTheme {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.background,
        ) {
            Box(modifier = Modifier.fillMaxSize().safeDrawingPadding()) {
                when (val current = screen) {
                    AppScreen.Loading -> LoadingScreen()

                    AppScreen.Onboarding ->
                        OnboardingScreen(
                            oauthLinking = oauthLinking,
                            oauthLinkingError = oauthLinkingError,
                            onLinked = { info ->
                                scope.launch {
                                    AppSessionHolder.openLibrary(context, info.id) { screen = it }
                                }
                            },
                        )

                    is AppScreen.Unlock ->
                        UnlockScreen(
                            libraryId = current.libraryId,
                            libraryName = current.libraryName,
                            fingerprint = current.fingerprint,
                            onUnlocked = {
                                scope.launch {
                                    AppSessionHolder.openLibrary(
                                        context,
                                        current.libraryId,
                                    ) { screen = it }
                                }
                            },
                        )

                    is AppScreen.LibraryOpen ->
                        LibraryScreen(
                            session = current.session,
                            onLeaveLibrary = {
                                scope.launch {
                                    AppSessionHolder.forgetActiveLibrary(context) {
                                        screen = it
                                    }
                                }
                            },
                        )

                    is AppScreen.Failed -> FailedScreen(message = current.message)
                }
            }
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
