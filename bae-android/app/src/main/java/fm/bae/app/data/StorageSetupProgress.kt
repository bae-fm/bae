package fm.bae.app.data

import fm.bae.app.BugReports
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.feed
import fm.bae.fastvalues.only
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeStorageSetupProgress

/**
 * One storage setup as the app follows it: the bridge's [setup], which the
 * setup call is handed, and its [steps], for the screen that shows them.
 * Make one for each call, and [close] it once the call ends. The steps are
 * pulled on [host]'s runtime, in [scope]; a failed pull is a bug, reported to
 * [bugs].
 */
internal class StorageSetupProgress(
    host: BridgeHost,
    private val scope: CoroutineScope,
    private val bugs: BugReports,
) : AutoCloseable {
    val setup = BridgeLibrarySetup(host)
    private val stream = LatestStream.storageSetupProgress()
    private val feed = stream.feed(setup, scope) { bugs.report("read a storage setup's steps", it) }
    val steps: LatestSource<BridgeStorageSetupProgress> = stream.only

    override fun close() {
        feed.cancel()
        setup.close()
    }
}
