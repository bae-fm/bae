package fm.bae.app.data

import androidx.annotation.VisibleForTesting
import fm.bae.fastvalues.LatestFeed
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.feed
import fm.bae.fastvalues.only
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeLibrarySetup

/**
 * One restore or join as the app follows it: the bridge's [setup], which the
 * call is handed, and where it stands, its [statuses], for the screen that
 * shows them. Make one for each call, and [close] it once the call ends.
 */
internal class LibrarySetupStatus private constructor(
    val setup: BridgeLibrarySetup,
    stream: LatestStream<BridgeBootstrapStatus>,
    private val feed: LatestFeed?,
) : AutoCloseable {
    val statuses: LatestSource<BridgeBootstrapStatus> = stream.only

    override fun close() {
        feed?.cancel()
        setup.close()
    }

    companion object {
        /** A setup whose stream reports a throw from its activity callback to [diagnostics], the process's sink. */
        fun of(diagnostics: BridgeDiagnostics): LibrarySetupStatus {
            val setup = BridgeLibrarySetup(diagnostics)
            val stream = LatestStream.bootstrapStatus()
            return LibrarySetupStatus(setup, stream, stream.feed(setup))
        }

        /** A test's: [setup] stands in for the bridge's, and the test writes [stream] in place of core. */
        @VisibleForTesting
        fun written(
            setup: BridgeLibrarySetup,
            stream: LatestStream<BridgeBootstrapStatus>,
        ): LibrarySetupStatus = LibrarySetupStatus(setup, stream, null)
    }
}
