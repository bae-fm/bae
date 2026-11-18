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
    /** The read's latest answer; waiting while the view is not shown. */
    private var read by mutableStateOf<LiveQueryState<BridgePlaylistListing>>(LiveQueryState.Waiting)

    /** The value the read last delivered; null until the first and after a failure. */
    val listing: BridgePlaylistListing? get() = read.answer

    override val state: ListPageState
        get() =
            when (val answer = read) {
                LiveQueryState.Waiting -> {
                    ListPageState.Waiting
                }

                is LiveQueryState.Failed -> {
                    val headline = appContext.getString(R.string.library_load_failed)
                    ListPageState.Failed(PageError(headline, answer.error, errors, ::retry))
                }

                is LiveQueryState.Answered -> {
                    ListPageState.Counted(answer.value.total.toInt())
                }
            }
    private var deliveries: ScreenSubscription<BridgePlaylistListing>? = null

    fun activate() {
        if (deliveries != null) return
        read = LiveQueryState.Waiting
        deliveries =
            ScreenSubscription(
                open(),
                scope,
                onValue = { value -> read = LiveQueryState.Answered(value) },
                onError = { value ->
                    logger.error("playlists read failed", value)
                    read = LiveQueryState.Failed(value)
                },
            )
    }

    fun deactivate() {
        deliveries?.stop()
        deliveries = null
        read = LiveQueryState.Waiting
    }

    private fun retry() {
        deactivate()
        activate()
    }
}
