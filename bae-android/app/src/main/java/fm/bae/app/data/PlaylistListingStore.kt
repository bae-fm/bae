package fm.bae.app.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.ErrorDisplays
import fm.bae.app.R
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaylistListing

private val logger = BaeLogger("bae.PlaylistListingStore")

/**
 * The Playlists view's one live read, open while the view is shown: every playlist this device
 * holds, whole, since the list is as long as the playlists the household made and is not read in
 * windows. A failure replaces the listing until the read answers again, or [retry] opens another.
 */
internal class PlaylistListingStore(
    private val open: () -> PulledSubscription<BridgePlaylistListing>,
    private val appContext: Context,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
) : BrowserPageState {
    /** The value the read last delivered; null until the first and after a failure. */
    var listing by mutableStateOf<BridgePlaylistListing?>(null)
        private set
    override var state by mutableStateOf<ListPageState>(ListPageState.Waiting)
        private set

    /** How many playlists the listing last said there are; 0 until it answers and after it fails. */
    val totalCount: Int get() = listing?.total?.toInt() ?: 0
    private var deliveries: ScreenSubscription<BridgePlaylistListing>? = null

    fun activate() {
        if (deliveries != null) return
        state = ListPageState.Waiting
        deliveries =
            ScreenSubscription(
                open(),
                scope,
                onValue = { value ->
                    listing = value
                    state = ListPageState.Counted(value.total.toInt())
                },
                onError = ::fail,
            )
    }

    fun deactivate() {
        deliveries?.stop()
        deliveries = null
        listing = null
        state = ListPageState.Waiting
    }

    private fun retry() {
        deactivate()
        activate()
    }

    private fun fail(value: BridgeException) {
        logger.error("playlists read failed", value)
        listing = null
        val headline = appContext.getString(R.string.library_load_failed)
        state = ListPageState.Failed(PageError(headline, value, errors, ::retry))
    }
}
