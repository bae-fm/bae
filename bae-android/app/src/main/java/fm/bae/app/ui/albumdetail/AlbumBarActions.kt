package fm.bae.app.ui.albumdetail

import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import fm.bae.app.OpenLibrary
import fm.bae.app.ui.components.PageDeletion
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeReleaseDownloadStatus
import uniffi.bae_bridge.BridgeReleasePlayability

/** The album's menu at the end of the page's bar, for the release in view. */
@Composable
internal fun AlbumBarActions(
    session: OpenLibrary,
    detail: BridgeAlbumDetail,
    callbacks: AlbumDetailCallbacks,
    deletion: PageDeletion<BridgeAlbum>,
    unpin: ReleaseUnpin,
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
    // A release this device can't play has nothing to pin, and one being pinned has Cancel on the page.
    val pin =
        if (!canPlayHere) {
            null
        } else {
            when (shown.downloadStatus) {
                BridgeReleaseDownloadStatus.Available -> {
                    PinMenuItem(pinned = false) {
                        session.messages.perform("queue release download") {
                            session.appHandle.queuePinReleases(listOf(shown.id))
                        }
                    }
                }

                BridgeReleaseDownloadStatus.Downloaded -> {
                    PinMenuItem(pinned = true) { unpin.request(shown.id, asksBeforeUnpinning) }
                        .takeUnless { unpin.unpinning }
                }

                else -> {
                    null
                }
            }
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
            onRate = onRate,
            pin = pin,
            onSetPrimaryRelease = setPrimary,
            onDelete = { deletion.ask(detail.album) },
        ),
    )
}
