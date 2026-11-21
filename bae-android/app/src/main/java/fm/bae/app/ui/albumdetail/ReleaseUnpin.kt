package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandleInterface

private val logger = BaeLogger("bae.ReleaseUnpin")

/**
 * Unpinning a release from the album page: the release whose confirmation is open, whether an unpin
 * is under way, and why the last one failed. Its files leave this phone; the release stays in the cloud.
 */
internal class ReleaseUnpin(
    private val appHandle: AppHandleInterface,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
) {
    /** The release the open confirmation names; null while it is closed. */
    var asking by mutableStateOf<String?>(null)
        private set

    var unpinning by mutableStateOf(false)
        private set

    var error by mutableStateOf<DisplayError?>(null)
        private set

    /** The person chose Unpin on [releaseId]: ask first while [asksBeforeUnpinning], else unpin at once. */
    fun request(
        releaseId: String,
        asksBeforeUnpinning: Boolean,
    ) {
        if (asksBeforeUnpinning) asking = releaseId else unpin(releaseId, stopAsking = false)
    }

    /** Close the confirmation without unpinning. */
    fun dismiss() {
        asking = null
    }

    /** Close the confirmation and unpin the release it named, first turning the asking off with [stopAsking]. */
    fun confirm(stopAsking: Boolean) {
        val releaseId = asking ?: return
        asking = null
        unpin(releaseId, stopAsking)
    }

    /**
     * Unpin [releaseId]. With [stopAsking], first turn "Ask before unpinning" off; if that write
     * fails the release stays pinned and the error is shown.
     */
    fun unpin(
        releaseId: String,
        stopAsking: Boolean,
    ) {
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

/** The unpinning the album page holds, run in the screen's scope. */
@Composable
internal fun rememberReleaseUnpin(session: OpenLibrary): ReleaseUnpin {
    val scope = rememberScreenScope()
    val errors = session.errors
    return remember(session, scope) { ReleaseUnpin(session.appHandle, errors, scope) }
}

/** Shows why the last unpin failed, and the confirmation while one is open. */
@Composable
internal fun ReleaseUnpinShown(unpin: ReleaseUnpin) {
    unpin.error?.let {
        AndroidNotice(
            error = it,
            modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        )
    }
    if (unpin.asking != null) UnpinDialog(onConfirm = unpin::confirm, onDismiss = unpin::dismiss)
}
