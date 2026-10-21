package fm.bae.app.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import fm.bae.app.OpenLibrary

/**
 * [content], one of [session]'s screens, counted as shown while composed:
 * the session closes its handle only once none is, so no tap on a screen
 * that is going away reaches a closed library.
 */
@Composable
internal fun SessionScreen(
    session: OpenLibrary,
    content: @Composable () -> Unit,
) {
    DisposableEffect(session) {
        session.screenShown()
        onDispose { session.screenGone() }
    }
    content()
}
