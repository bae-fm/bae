package fm.bae.app.ui.onboarding

import fm.bae.app.playback.unfaked
import uniffi.bae_bridge.BootstrapCancellation
import uniffi.bae_bridge.NoHandle

/** A restore's stop with no Rust object behind it, for a flow whose host is a fake; stopping it is not faked. */
internal class FakeBootstrapCancellation : BootstrapCancellation(NoHandle) {
    override fun cancel() {
        unfaked("BootstrapCancellation.cancel")
    }
}
