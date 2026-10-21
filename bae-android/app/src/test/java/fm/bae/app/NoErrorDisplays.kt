package fm.bae.app

import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaybackErrorReason

/** Error lines for tests without the native catalog: each names the failure it shows. */
internal object NoErrorDisplays : ErrorDisplays {
    override fun display(reason: BridgePlaybackErrorReason) = DisplayError("playback failed", reason.diagnosticDetail)

    override fun display(error: BridgeException) = DisplayError("failed", error.diagnosticDetail)
}
