package fm.bae.app.ui.settings

import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome

/** Nothing a scene does fails, so no failure shows. */
internal object NothingFails : ErrorDisplays {
    override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

    override fun display(error: BridgeException): DisplayError? = null

    override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

    override fun display(outcome: BridgePlayOutcome): DisplayError? = null
}
