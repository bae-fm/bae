package fm.bae.app.ui.albumdetail

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandleInterface

private val logger = BaeLogger("bae.ReleaseUnpin")

/**
 * Unpinning the release [releaseId] from its page: whether it is under way, and why the last
 * one failed. Its files leave this phone; the release stays in the cloud.
 */
internal class ReleaseUnpin(
    private val releaseId: String,
    private val appHandle: AppHandleInterface,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
) {
    var unpinning by mutableStateOf(false)
        private set

    var error by mutableStateOf<DisplayError?>(null)
        private set

    /**
     * Unpin the release. With [stopAsking], first turn "Ask before unpinning" off; if that write
     * fails the release stays pinned and the error is shown.
     */
    fun unpin(stopAsking: Boolean) {
        error = null
        unpinning = true
        scope.launch {
            try {
                performBridgeAction(logger, "unpin release $releaseId", errors, { error = it }) {
                    if (stopAsking) appHandle.setAskBeforeUnpinning(false)
                    appHandle.unpinReleases(listOf(releaseId))
                }
            } finally {
                unpinning = false
            }
        }
    }
}

/** The unpinning of the release [releaseId] the page shows, run in the screen's scope. */
@Composable
internal fun rememberReleaseUnpin(
    session: OpenLibrary,
    releaseId: String,
): ReleaseUnpin {
    val scope = rememberScreenScope()
    val errors = session.errors
    return remember(releaseId, session, scope) { ReleaseUnpin(releaseId, session.appHandle, errors, scope) }
}
