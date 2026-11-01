package fm.bae.app.data

import fm.bae.app.BugReports
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.feed
import fm.bae.fastvalues.only
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeSetupProgress

/**
 * One setup call as the app follows it: the bridge's [setup], which the call
 * is handed, and its [steps], for the screen that shows them. Make one for
 * each call, and [close] it once the call ends. The steps are pulled on
 * [host]'s runtime, in [scope]; a failed pull is a bug, reported to [bugs].
 */
internal class LibrarySetupProgress(
    host: BridgeHost,
    scope: CoroutineScope,
    bugs: BugReports,
) : AutoCloseable {
    val setup = BridgeLibrarySetup(host)
    private val stream = LatestStream.setupProgress()
    private val feed = stream.feed(setup, scope) { bugs.report("read a setup call's steps", it) }
    val steps: LatestSource<BridgeSetupProgress> = stream.only

    override fun close() {
        feed.cancel()
        setup.close()
    }
}
