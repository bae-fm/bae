package fm.bae.app.data

import fm.bae.app.BugReports
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.feed
import fm.bae.fastvalues.only
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeClosingProgress
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeLibraryClosing

/**
 * One close of a library as the app follows it: the bridge's [closing], which
 * the close is handed, and its [steps], for the screen that shows them. Make
 * one for each close the app shows, and [close] it once the close ends. The
 * steps are pulled on [host]'s runtime, which runs on after the library's own
 * has ended, in [scope]; a failed pull is a bug, reported to [bugs].
 */
internal class LibraryClosingSteps(
    host: BridgeHost,
    scope: CoroutineScope,
    bugs: BugReports,
) : AutoCloseable {
    val closing = BridgeLibraryClosing(host)
    private val stream = LatestStream.closingSteps()
    private val feed = stream.feed(closing, scope) { bugs.report("read a library close's steps", it) }
    val steps: LatestSource<BridgeClosingProgress> = stream.only

    override fun close() {
        feed.cancel()
        closing.close()
    }
}
