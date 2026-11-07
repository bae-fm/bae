package fm.bae.app.data

import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import uniffi.bae_bridge.BridgeUploadTransfer

/**
 * The stream of the files being sent together, which the bridge holds and the
 * View drawing it reads once per display frame while files are being sent.
 */
class OutboxStore(
    /** The files being sent together while any is: the session's to feed, or a preview's fixed value. */
    val filesBeingSent: LatestStream<BridgeUploadTransfer> = LatestStream.uploadTotal(),
) {
    /** The latest of the files being sent together, while any is, for the View that reads it each frame. */
    val totalProgress: LatestSource<BridgeUploadTransfer> get() = filesBeingSent.only
}
