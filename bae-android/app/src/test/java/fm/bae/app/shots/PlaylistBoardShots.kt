package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.BridgeFixtures
import fm.bae.app.R
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.library.AndroidDetailScreen
import fm.bae.app.ui.library.PlaylistContent
import fm.bae.app.ui.library.PlaylistEntryActions
import fm.bae.app.ui.library.PlaylistOptionsMenu
import fm.bae.app.ui.library.PlaylistOwnerActions
import fm.bae.app.ui.library.SHARING_CHOICES
import fm.bae.app.ui.library.deletePlaylistParts
import fm.bae.app.ui.library.playlistDialogParts
import org.junit.Test
import uniffi.bae_bridge.BridgePersonRef
import uniffi.bae_bridge.BridgePlaylistCover
import uniffi.bae_bridge.BridgeSharing

/** The shots of a playlist's page and the dialogs it opens, drawn as the boards draw them. */
class PlaylistBoardShots : BoardShot() {
    private val entries =
        lateNightTracks.mapIndexed { index, track ->
            BridgeFixtures.playlistEntry(
                "position-$index",
                track.title,
                canMoveUp = index > 0,
                canMoveDown = index < lateNightTracks.lastIndex,
                artist = track.artist,
                album = track.album.title,
                durationMs = track.durationMs,
            )
        }

    private val detail =
        BridgeFixtures.playlistDetail(
            BridgeFixtures.playlistSummary(name = "Late night", sharing = BridgeSharing.HOUSEHOLD).copy(
                owner = BridgePersonRef(person = "person-ana", name = "Ana", initials = "A", avatarVersion = null),
                cover = BridgePlaylistCover.Grid(covers = listOf(null, null, null, null)),
            ),
            entries,
        )

    private val actions =
        PlaylistEntryActions(
            onPlay = {},
            onPlayNext = {},
            onAddToQueue = {},
            onAddToPlaylist = {},
            onRemove = {},
            onMove = { _, _, _ -> },
        )

    @Composable
    private fun Page() {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            AndroidDetailScreen(
                onBack = {},
                backLabel = stringResource(R.string.back_to_library),
                nowPlayingBar = { Bar(playingLowTideHours) },
                actions = { PlaylistOptionsMenu(onShuffle = {}, owner = PlaylistOwnerActions(onEdit = {}, onDelete = {})) },
            ) {
                Box(modifier = Modifier.weight(1f).fillMaxWidth()) { PlaylistContent(detail = detail, actions = actions) }
            }
        }
    }

    /** A playlist's page: its picture, owner, count and who sees it, Play under them, and its tracks. */
    @Test
    fun androidPlaylistsPlaylist() = shot("android.playlists.playlist") { ScreenScene { Page() } }

    /** The Edit playlist dialog over the page, showing the playlist's name and who sees it. */
    @Test
    fun androidPlaylistsEdit() =
        shot("android.playlists.edit") {
            InPlaceDialogScene(behind = { Page() }) {
                playlistDialogParts(
                    title = stringResource(R.string.playlist_edit_title),
                    confirmLabel = stringResource(R.string.save),
                    name = "Late night",
                    sharing = SHARING_CHOICES.last(),
                    error = null,
                    canConfirm = true,
                    onNameChange = {},
                    onSharingChange = {},
                    onConfirm = {},
                    onDismiss = {},
                )
            }
        }

    /** The confirmation bae shows before it deletes a playlist. */
    @Test
    fun androidPlaylistsDelete() =
        shot("android.playlists.delete") {
            InPlaceDialogScene(behind = { Page() }) { deletePlaylistParts("Late night", onConfirm = {}, onDismiss = {}) }
        }
}
