package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.runtime.Composable
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
import fm.bae.app.factsLine
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSheet

/**
 * Adding a track to a playlist from a track's menu: whether the menu offers it (the person can make
 * playlists) and the track whose sheet of playlists is open, if any.
 */
internal class TrackPlaylistAdd(
    val offered: Boolean,
    /** The track whose Add to playlist sheet is open. */
    val trackId: String?,
    private val choose: (String?) -> Unit,
) {
    /** Open the sheet for [trackId]. */
    fun ask(trackId: String) = choose(trackId)

    /** Close the sheet. */
    fun dismiss() = choose(null)
}

/** The Add to playlist state of a page with tracks; the playlists are read while the page is shown. */
@Composable
internal fun rememberTrackPlaylistAdd(session: OpenLibrary): TrackPlaylistAdd {
    val page = rememberPlaylistListing(session)
    var trackId by remember { mutableStateOf<String?>(null) }
    return TrackPlaylistAdd(page.listing?.canCreate == true, trackId) { trackId = it }
}

/** The sheet of the person's playlists for the track [add] holds, with New playlist… first. */
@Composable
internal fun TrackPlaylistAddShown(
    session: OpenLibrary,
    add: TrackPlaylistAdd,
) {
    val trackId = add.trackId ?: return
    var creating by remember(trackId) { mutableStateOf(false) }
    val context = LocalContext.current
    val targets =
        session.browserPages.playlists.listing
            ?.addTargets
            .orEmpty()
    AndroidSheet(onDismiss = add::dismiss, title = stringResource(R.string.playlist_add_title)) {
        Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
            AndroidRow(
                title = stringResource(R.string.playlist_new_row),
                action = true,
                leadingIcon = Icons.Filled.Add,
                onClick = { creating = true },
            )
            targets.forEach { playlist ->
                AndroidLibraryRow(
                    title = playlist.name,
                    subtitle =
                        context.factsLine(
                            listOf(
                                context.coreString(playlist.trackCount),
                                stringResource(playlist.sharing.labelRes()),
                            ),
                        ),
                    playlistCover = playlist.cover,
                    onClick = {
                        session.messages.perform("add track $trackId to playlist ${playlist.id}") {
                            session.appHandle.appendPlaylistEntry(playlist.id, trackId)
                        }
                        add.dismiss()
                    },
                )
            }
        }
    }
    if (creating) {
        NewPlaylistDialog(
            session = session,
            trackIds = listOf(trackId),
            onDismiss = { creating = false },
            onCreated = { add.dismiss() },
        )
    }
}
