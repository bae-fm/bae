package fm.bae.app.data

import fm.bae.app.BugReports
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.feed
import fm.bae.fastvalues.only
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.pullEach
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeStorageSetupProgress

/**
 * One storage setup as the app follows it: the bridge's [setup], which the
 * setup call is handed, its [steps], for the screen that shows them, and
 * whether it is past its point of no return, pulled each time that changes
 * through [followFinishing]. Make one for each call, and [close] it once the
 * call ends. The steps and the pulls run on [host]'s runtime, in [scope]; a
 * failed pull is a bug, reported to [bugs].
 */
internal class StorageSetupProgress(
    host: BridgeHost,
    private val scope: CoroutineScope,
    private val bugs: BugReports,
) : AutoCloseable {
    val setup = BridgeLibrarySetup(host)
    private val stream = LatestStream.storageSetupProgress()
    private val feed = stream.feed(setup, scope) { bugs.report("read a storage setup's steps", it) }
    private val finishing: PulledSubscription<Boolean> =
        setup.subscribeStorageFinishing().let { pulled(it::next, it::cancel) }
    val steps: LatestSource<BridgeStorageSetupProgress> = stream.only

    /**
     * Hand [onFinishing] whether the setup is past its point of no return,
     * creating the library in storage or recording changed access, each time
     * that changes, in the calling coroutine, until that coroutine is
     * cancelled or the setup is closed. Past it, coven finishes the setup
     * rather than undoing it, so its screen offers no Cancel.
     */
    suspend fun followFinishing(onFinishing: (Boolean) -> Unit) {
        finishing.pullEach(
            onValue = { onFinishing(it) },
            onError = { bugs.report("read whether a storage setup is finishing", it) },
        )
    }

    override fun close() {
        feed.cancel()
        // Ends even when the screen's scope is closing.
        scope.launch(NonCancellable) { finishing.cancel() }
        setup.close()
    }
}
