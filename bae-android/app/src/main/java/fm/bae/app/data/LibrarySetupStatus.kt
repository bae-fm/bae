package fm.bae.app.data

import androidx.annotation.VisibleForTesting
import fm.bae.app.BugReports
import fm.bae.fastvalues.LatestFeed
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
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeBootstrapStep
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeLibrarySetup

/**
 * One restore, join or reset as the app follows it: the bridge's [setup],
 * which the call is handed, where it stands, pulled each time that changes
 * through [followStates], and the counted step it runs, [steps], for the
 * View that draws it each display frame. Make one for each call, and [close]
 * it once the call ends.
 */
internal class LibrarySetupStatus private constructor(
    val setup: BridgeLibrarySetup,
    private val states: PulledSubscription<BridgeBootstrapState>,
    stream: LatestStream<BridgeBootstrapStep>,
    private val feed: LatestFeed?,
    private val scope: CoroutineScope,
    private val reportBug: (BridgeException) -> Unit,
) : AutoCloseable {
    val steps: LatestSource<BridgeBootstrapStep> = stream.only

    /**
     * Hand [onState] each state the call says, in the calling coroutine,
     * until that coroutine is cancelled or the setup is closed.
     */
    suspend fun followStates(onState: (BridgeBootstrapState) -> Unit) {
        states.pullEach(onValue = { onState(it) }, onError = { reportBug(it) })
    }

    override fun close() {
        feed?.cancel()
        // Ends even when the session's scope is closing.
        scope.launch(NonCancellable) { states.cancel() }
        setup.close()
    }

    companion object {
        /**
         * A setup whose states and steps are pulled on [host]'s runtime, in
         * [scope]; a failed pull is a bug, reported to [bugs].
         */
        fun of(
            host: BridgeHost,
            scope: CoroutineScope,
            bugs: BugReports,
        ): LibrarySetupStatus {
            val setup = BridgeLibrarySetup(host)
            val stream = LatestStream.bootstrapSteps()
            val feed = stream.feed(setup, scope) { bugs.report("read a setup's steps", it) }
            val states = setup.subscribeStates().let { pulled(it::next, it::cancel) }
            return LibrarySetupStatus(setup, states, stream, feed, scope) { bugs.report("read a setup's states", it) }
        }

        /**
         * A test's: [setup] stands in for the bridge's, [states] answer the
         * pulls in place of core, and the test writes [stream] in place of
         * core.
         */
        @VisibleForTesting
        fun written(
            setup: BridgeLibrarySetup,
            states: PulledSubscription<BridgeBootstrapState>,
            stream: LatestStream<BridgeBootstrapStep>,
            scope: CoroutineScope,
        ): LibrarySetupStatus =
            LibrarySetupStatus(setup, states, stream, null, scope) { error ->
                throw AssertionError("a test's states failed", error)
            }
    }
}
