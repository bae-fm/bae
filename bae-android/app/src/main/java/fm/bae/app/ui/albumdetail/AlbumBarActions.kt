package fm.bae.app.ui.albumdetail

import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.OpenLibrary
import fm.bae.app.heartersText
import fm.bae.app.ui.components.PageDeletion
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeRatingTarget
import uniffi.bae_bridge.BridgeReleasePlayability
import uniffi.bae_bridge.BridgeReleaseStorageAction

/**
 * The heart the person chose for an album, which the page keeps showing until the write returns: [shown] is the
 * chosen heart while the write runs, and null otherwise. A choice made while a write runs is ignored.
 */
internal class AlbumHeartChoice(
    private val session: OpenLibrary,
    private val albumId: String,
    private val scope: CoroutineScope,
) {
    var shown by mutableStateOf<Boolean?>(null)
        private set

    fun choose(hearted: Boolean) {
        if (shown != null) return
        shown = hearted
        scope.launch {
            try {
                session.messages.performAndWait("setHearted album $albumId") {
                    session.appHandle.setHearted(BridgeRatingTarget.Album(albumId), hearted)
                }
            } finally {
                shown = null
            }
        }
    }
}

@Composable
internal fun rememberAlbumHeart(
    session: OpenLibrary,
    albumId: String,
): AlbumHeartChoice {
    val scope = rememberScreenScope()
    return remember(session, albumId, scope) { AlbumHeartChoice(session, albumId, scope) }
}

/** The album's menu at the end of the page's bar, for the release in view. */
@Composable
internal fun AlbumBarActions(
    session: OpenLibrary,
    detail: BridgeAlbumDetail,
    callbacks: AlbumDetailCallbacks,
    deletion: PageDeletion<BridgeAlbum>,
    unpin: ReleaseUnpin,
    heartChoice: AlbumHeartChoice,
    onRate: () -> Unit,
) {
    val shown = detail.shownRelease
    val canPlayHere = shown.playability !is BridgeReleasePlayability.OnOtherDevice
    val asksBeforeUnpinning =
        session.configStore.config
            .collectAsState()
            .value.askBeforeUnpinning
    val setPrimary =
        if (detail.shownReleaseCanBecomePrimary) {
            {
                session.messages.perform("setPrimaryRelease ${shown.id}") {
                    session.appHandle.setPrimaryRelease(detail.album.id, shown.id)
                }
            }
        } else {
            null
        }
    // Core lists Pin or Unpin only while the release can take it: one being pinned has Cancel on the page.
    val pin =
        when {
            BridgeReleaseStorageAction.PIN in shown.storageActions -> {
                PinMenuItem(pinned = false) {
                    session.messages.perform("queue release download") {
                        session.appHandle.queuePinReleases(listOf(shown.id))
                    }
                }
            }

            BridgeReleaseStorageAction.UNPIN in shown.storageActions && !unpin.unpinning -> {
                PinMenuItem(pinned = true) { unpin.request(shown.id, asksBeforeUnpinning) }
            }

            else -> {
                null
            }
        }
    val context = LocalContext.current
    val ratingAction =
        when (val rating = detail.rating) {
            BridgeAlbumRating.Hidden -> null
            is BridgeAlbumRating.Stars -> AlbumRatingAction.Rate(onRate)
            is BridgeAlbumRating.Heart ->
                AlbumRatingAction.Heart(
                    hearters = context.heartersText(rating),
                    hearted = heartChoice.shown ?: rating.hearted,
                    onChoose = heartChoice::choose,
                )
        }
    AlbumOptionsMenu(
        AlbumMenuActions(
            play =
                if (canPlayHere) {
                    PlayMenuActions(
                        onShuffle = callbacks.onShuffleRelease,
                        onPlayNext = callbacks.onPlayReleaseNext,
                        onAddToQueue = callbacks.onAddReleaseToQueue,
                    )
                } else {
                    null
                },
            rating = ratingAction,
            pin = pin,
            onSetPrimaryRelease = setPrimary,
            onDelete = { deletion.ask(detail.album) },
        ),
    )
}
