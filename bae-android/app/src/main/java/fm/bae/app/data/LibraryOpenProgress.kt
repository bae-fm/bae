package fm.bae.app.data

import fm.bae.app.BugReports
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.feed
import fm.bae.fastvalues.only
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeLibraryOpening
import uniffi.bae_bridge.BridgeOpenProgress

/**
 * One open of a library as the app follows it: the bridge's [opening], which
 * the call that opens the library is handed, and its [steps], for the View
 * that draws them. Make one for each call, and [close] it once the call ends.
 * The steps are pulled on [host]'s runtime, in [scope]; a failed pull is a bug,
 * reported to [bugs].
 */
internal class LibraryOpenProgress(
    host: BridgeHost,
    scope: CoroutineScope,
    bugs: BugReports,
) : AutoCloseable {
    val opening = BridgeLibraryOpening(host)
    private val stream = LatestStream.openProgress()
    private val feed = stream.feed(opening, scope) { bugs.report("read a library open's steps", it) }
    val steps: LatestSource<BridgeOpenProgress> = stream.only

    override fun close() {
        feed.cancel()
        opening.close()
    }
}
