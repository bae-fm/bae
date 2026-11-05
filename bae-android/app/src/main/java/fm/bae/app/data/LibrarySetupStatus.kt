package fm.bae.app.data

import androidx.annotation.VisibleForTesting
import fm.bae.app.BugReports
import fm.bae.fastvalues.LatestFeed
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.feed
import fm.bae.fastvalues.only
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeLibrarySetup

/**
 * One restore or join as the app follows it: the bridge's [setup], which the
 * call is handed, and where it stands, its [states], for the screen that
 * shows them. Make one for each call, and [close] it once the call ends.
 */
internal class LibrarySetupStatus private constructor(
    val setup: BridgeLibrarySetup,
    stream: LatestStream<BridgeBootstrapState>,
    private val feed: LatestFeed?,
) : AutoCloseable {
    val states: LatestSource<BridgeBootstrapState> = stream.only

    override fun close() {
        feed?.cancel()
        setup.close()
    }

    companion object {
        /**
         * A setup whose states are pulled on [host]'s runtime, in [scope]; a failed pull is a bug,
         * reported to [bugs].
         */
        fun of(
            host: BridgeHost,
            scope: CoroutineScope,
            bugs: BugReports,
        ): LibrarySetupStatus {
            val setup = BridgeLibrarySetup(host)
            val stream = LatestStream.bootstrapState()
            val feed = stream.feed(setup, scope) { bugs.report("read a setup's states", it) }
            return LibrarySetupStatus(setup, stream, feed)
        }

        /** A test's: [setup] stands in for the bridge's, and the test writes [stream] in place of core. */
        @VisibleForTesting
        fun written(
            setup: BridgeLibrarySetup,
            stream: LatestStream<BridgeBootstrapState>,
        ): LibrarySetupStatus = LibrarySetupStatus(setup, stream, null)
    }
}
