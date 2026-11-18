package fm.bae.app.ui.onboarding

import fm.bae.app.playback.unfaked
import uniffi.bae_bridge.BridgeCancellation
import uniffi.bae_bridge.NoHandle

/** A restore's stop with no Rust object behind it, for a flow whose host is a fake; stopping it is not faked. */
internal class FakeBridgeCancellation : BridgeCancellation(NoHandle) {
    override fun cancel() {
        unfaked("BridgeCancellation.cancel")
    }
}
