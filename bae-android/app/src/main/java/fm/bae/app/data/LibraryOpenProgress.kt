package fm.bae.app.data

import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.feed
import fm.bae.fastvalues.only
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeLibraryOpening
import uniffi.bae_bridge.BridgeOpenProgress

/**
 * One open of a library as the app follows it: the bridge's [opening], which
 * the call that opens the library is handed, and its [steps], for the View
 * that draws them. Make one for each call, and [close] it once the call ends.
 * A throw while the steps are read is reported to [diagnostics], the
 * process's sink.
 */
internal class LibraryOpenProgress(
    diagnostics: BridgeDiagnostics,
) : AutoCloseable {
    val opening = BridgeLibraryOpening(diagnostics)
    private val stream = LatestStream.openProgress()
    private val feed = stream.feed(opening)
    val steps: LatestSource<BridgeOpenProgress> = stream.only

    override fun close() {
        feed.cancel()
        opening.close()
    }
}
