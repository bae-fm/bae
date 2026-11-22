package fm.bae.app.data

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BugReports
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeRatingSystems

/**
 * How the library shows the ratings of its albums and of its tracks, for as long as the library is open. The value
 * stays null until the read answers, and the screens draw no rating until then. A failed read is a bug, reported to the
 * person by the snackbar, and leaves the value it had.
 */
internal class RatingSystemsStore(
    private val appHandle: AppHandleInterface,
    private val scope: CoroutineScope,
    private val bugs: BugReports,
) {
    var systems by mutableStateOf<BridgeRatingSystems?>(null)
        private set
    private var subscription: ScreenSubscription<BridgeRatingSystems>? = null

    fun start() {
        check(subscription == null) { "the rating systems' read started twice" }
        subscription =
            ScreenSubscription(
                appHandle.subscribeRatingSystems().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { systems = it },
                onError = { bugs.report("read the library's rating systems", it) },
            )
    }

    fun stop() {
        subscription?.stop()
        subscription = null
    }
}
