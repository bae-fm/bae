package fm.bae.app.data

import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.feed
import fm.bae.fastvalues.only
import uniffi.bae_bridge.BridgeLibraryOpening
import uniffi.bae_bridge.BridgeOpenProgress

/**
 * One open of a library as the app follows it: the bridge's [opening], which
 * the call that opens the library is handed, and its [steps], for the View
 * that draws them. Make one for each call, and [close] it once the call ends.
 */
internal class LibraryOpenProgress : AutoCloseable {
    val opening = BridgeLibraryOpening()
    private val stream = LatestStream.openProgress()
    private val feed = stream.feed(opening)
    val steps: LatestSource<BridgeOpenProgress> = stream.only

    override fun close() {
        feed.cancel()
        opening.close()
    }
}
