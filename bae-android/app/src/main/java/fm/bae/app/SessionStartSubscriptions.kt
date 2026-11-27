package fm.bae.app

import fm.bae.screensubscription.StartedSubscription
import fm.bae.screensubscription.pulled
import fm.bae.screensubscription.start
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeDownloadCounts

/**
 * The subscriptions an open library's stores are made with the first values of: the settings, the download
 * counts, and whether files move only on unmetered networks. The session goes on pulling the same ones, so
 * a change made while it opens reaches the stores. Closing the library ends them.
 */
internal class SessionStartSubscriptions(
    val config: StartedSubscription<BridgeConfig>,
    val downloadCounts: StartedSubscription<BridgeDownloadCounts>,
    val unmeteredOnly: StartedSubscription<Boolean>,
) {
    companion object {
        /** Subscribe on [handle] and wait for each first value. A first read that fails throws its failure. */
        suspend fun start(handle: AppHandleInterface) =
            SessionStartSubscriptions(
                config = handle.subscribeConfig().let { pulled(it::next, it::cancel) }.start(),
                downloadCounts = handle.subscribeDownloadCounts().let { pulled(it::next, it::cancel) }.start(),
                unmeteredOnly = handle.subscribeUnmeteredOnly().let { pulled(it::next, it::cancel) }.start(),
            )
    }
}
