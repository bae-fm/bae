package fm.bae.app

import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome

/** Error lines for tests without the native catalog: each names the failure it shows. */
internal object NoErrorDisplays : ErrorDisplays {
    override fun display(reason: BridgePlaybackErrorReason) =
        DisplayError("playback failed", (reason as? BridgePlaybackErrorReason.Diagnostic)?.error?.diagnosticDetail)

    override fun display(error: BridgeException) = DisplayError("failed", error.diagnosticDetail)

    override fun display(outcome: BridgeSyncRetryOutcome) =
        when (outcome) {
            is BridgeSyncRetryOutcome.Synced -> null
            is BridgeSyncRetryOutcome.Failed -> display(outcome.error)
            else -> DisplayError("not synced")
        }

    override fun display(outcome: BridgePlayOutcome) =
        when (outcome) {
            BridgePlayOutcome.Playing -> null
            else -> DisplayError("not everything played")
        }
}
