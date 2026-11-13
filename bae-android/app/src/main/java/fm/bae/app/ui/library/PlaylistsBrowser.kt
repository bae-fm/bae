package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.PlaylistListingStore
import fm.bae.app.factsLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.components.ButtonKind

@Composable
internal fun rememberPlaylistListing(session: OpenLibrary): PlaylistListingStore {
    val page = session.browserPages.playlists
    DisposableEffect(page) {
        page.activate()
        onDispose(page::deactivate)
    }
    return page
}

/**
 * The Playlists view: every playlist this device holds, then a New playlist button where the person
 * can make one. With none, the button stands under the empty line; without a profile there is no
 * button, and the line says playlists wait for one.
 */
@Composable
internal fun PlaylistListContent(
    session: OpenLibrary,
    page: PlaylistListingStore,
    listState: LazyListState,
) {
    val context = LocalContext.current
    val listing = page.listing
    val canCreate = listing?.canCreate == true
    var creating by remember { mutableStateOf(false) }
    val newPlaylistButton: @Composable () -> Unit = {
        AndroidButton(onClick = { creating = true }, kind = ButtonKind.TONAL) {
            Text(stringResource(R.string.playlist_new))
        }
    }
    LibraryPageContent(
        session = session,
        page = page,
        emptyMessage =
            stringResource(
                if (canCreate) R.string.library_empty_playlists else R.string.library_empty_playlists_without_profile,
            ),
        emptyAction = if (canCreate) newPlaylistButton else null,
    ) {
        LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
            items(listing?.listed.orEmpty(), key = { it.id }) { playlist ->
                AndroidLibraryRow(
                    title = playlist.name,
                    subtitle =
                        context.factsLine(
                            listOf(
                                context.coreString(playlist.trackCount),
                                context.getString(playlist.sharing.labelRes()),
                            ),
                        ),
                    playlistCover = playlist.cover,
                )
            }
            if (canCreate) {
                item {
                    Box(modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related)) {
                        newPlaylistButton()
                    }
                }
            }
        }
    }
    if (creating) {
        NewPlaylistDialog(session = session, onDismiss = { creating = false })
    }
}
