package fm.bae.app.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import fm.bae.app.LocalBugHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.job

/**
 * A screen's coroutine scope: cancelled when the screen leaves composition,
 * and with the app's bug handler, so a coroutine that fails in a way nothing
 * caught is reported as a bug — "Something went wrong" — instead of crashing
 * the app. Each launch stands alone: one failing never cancels the others.
 */
@Composable
internal fun rememberScreenScope(): CoroutineScope {
    val handler = LocalBugHandler.current
    val composition = rememberCoroutineScope()
    return remember(composition, handler) {
        CoroutineScope(composition.coroutineContext + SupervisorJob(composition.coroutineContext.job) + handler)
    }
}
