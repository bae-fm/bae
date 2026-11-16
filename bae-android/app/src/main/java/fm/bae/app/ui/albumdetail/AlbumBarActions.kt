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
    selectedReleaseId: String?,
    deletion: PageDeletion<BridgeAlbum>,
) {
    val release = detail.chosenOrMainRelease(selectedReleaseId)
    val setPrimary =
        release?.takeIf { detail.showsReleaseChoices && it.id != detail.album.primaryReleaseId }?.let { chosen ->
            {
                session.messages.perform("setPrimaryRelease ${chosen.id}") {
                    session.appHandle.setPrimaryRelease(detail.album.id, chosen.id)
                }
            }
        }
    AlbumOptionsMenu(onSetPrimaryRelease = setPrimary, onDelete = { deletion.ask(detail.album) })
}
