package fm.bae.app.data

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BugReports
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgePersonProfile

/**
 * This device's person for as long as the library is open: their profile, whose picture ends the library's top
 * bar, and whether they are an admin, which the menu under the picture needs. Each stays null until its read
 * answers, and a failed read is a bug, reported to the person by the snackbar, and leaves the value it had; the
 * screen that draws the picture and the menu draws none while either is null.
 */
internal class PersonStore(
    private val appHandle: AppHandleInterface,
    private val scope: CoroutineScope,
    private val bugs: BugReports,
) {
    var profile by mutableStateOf<BridgePersonProfile?>(null)
        private set
    var isAdmin by mutableStateOf<Boolean?>(null)
        private set
    private var subscriptions = emptyList<ScreenSubscription<*>>()

    fun start() {
        check(subscriptions.isEmpty()) { "the person's reads started twice" }
        subscriptions =
            listOf(
                ScreenSubscription(
                    appHandle.subscribePeople().let { pulled(it::next, it::cancel) },
                    scope,
                    onValue = { people -> profile = people.own?.profile },
                    onError = { bugs.report("read this device's person", it) },
                ),
                ScreenSubscription(
                    appHandle.subscribeMembers().let { pulled(it::next, it::cancel) },
                    scope,
                    onValue = { membership -> isAdmin = membership.selfIsAdmin },
                    onError = { bugs.report("read whether this device's person is an admin", it) },
                ),
            )
    }

    fun stop() {
        subscriptions.forEach(ScreenSubscription<*>::stop)
        subscriptions = emptyList()
    }
}
