package fm.bae.app.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.R
import fm.bae.app.diagnosticDetail
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaylistListing

private val logger = BaeLogger("bae.PlaylistListingStore")

/**
 * The Playlists view's one live read, open while the view is shown: every playlist this device
 * holds, whole, since the list is as long as the playlists the household made and is not read in
 * windows. A failure closes the read and shows until [retry] opens another.
 */
internal class PlaylistListingStore(
    private val open: () -> PulledSubscription<BridgePlaylistListing>,
    private val appContext: Context,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
) : BrowserPageState {
    /** The last value the read delivered; null until the first. */
    var listing by mutableStateOf<BridgePlaylistListing?>(null)
        private set
    override var loading by mutableStateOf(true)
        private set
    override var error by mutableStateOf<PageError?>(null)
        private set
    override val hasRows: Boolean get() = totalCount > 0
    override val totalCount: Int get() = listing?.listed?.size ?: 0
    private var deliveries: ScreenSubscription<BridgePlaylistListing>? = null

    fun activate() {
        if (deliveries != null) return
        loading = true
        error = null
        deliveries =
            ScreenSubscription(
                open(),
                scope,
                onValue = { value ->
                    listing = value
                    loading = false
                    error = null
                },
                onError = ::fail,
            )
    }

    fun deactivate() {
        deliveries?.stop()
        deliveries = null
        listing = null
        loading = true
        error = null
    }

    private fun retry() {
        deactivate()
        activate()
    }

    private fun fail(value: BridgeException) {
        logger.error("playlists read failed", value)
        deliveries?.stop()
        deliveries = null
        loading = false
        // This app words the headline; core's diagnostic stays behind it for the disclosure.
        error =
            PageError(
                DisplayError(
                    appContext.getString(R.string.library_load_failed),
                    value.diagnosticDetail,
                    // A cancellation, which core shows nothing for, rules nothing out.
                    errors.display(value)?.offersRetry != false,
                ),
                ::retry,
            )
    }
}
