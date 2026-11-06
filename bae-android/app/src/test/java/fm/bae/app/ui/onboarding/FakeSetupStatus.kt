package fm.bae.app.ui.onboarding

import fm.bae.app.data.LibrarySetupStatus
import fm.bae.fastvalues.LatestStream
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.channels.Channel
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.NoHandle

/** A setup with no Rust object behind it, for a flow whose host is a fake, which writes nothing into it. */
internal class FakeLibrarySetup : BridgeLibrarySetup(NoHandle) {
    override fun close() = Unit
}

/** Where a fake host's restore or join stands: nothing, since a fake says no state. */
internal fun fakeSetupStatus(): LibrarySetupStatus =
    LibrarySetupStatus.written(
        FakeLibrarySetup(),
        pulled({ awaitCancellation() }, {}),
        LatestStream.written(),
        CoroutineScope(Dispatchers.Unconfined),
    )

/** States a test says in place of core: each [say] answers the next pull. */
internal class SaidStates : PulledSubscription<BridgeBootstrapState> {
    private val said = Channel<BridgeBootstrapState>(Channel.UNLIMITED)

    fun say(state: BridgeBootstrapState) {
        said.trySend(state)
    }

    override suspend fun next(): BridgeBootstrapState = said.receive()

    override suspend fun cancel() {
        said.close()
    }
}
