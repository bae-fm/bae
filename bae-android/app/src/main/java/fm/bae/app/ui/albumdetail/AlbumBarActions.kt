package fm.bae.app.ui.albumdetail

import androidx.compose.runtime.Composable
import fm.bae.app.OpenLibrary
import fm.bae.app.ui.components.PageDeletion
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail

/** The album's menu at the end of the page's bar, for the release in view. */
@Composable
internal fun AlbumBarActions(
    session: OpenLibrary,
    detail: BridgeAlbumDetail,
    deletion: PageDeletion<BridgeAlbum>,
) {
    val shown = detail.shownRelease
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
    AlbumOptionsMenu(onSetPrimaryRelease = setPrimary, onDelete = { deletion.ask(detail.album) })
}
