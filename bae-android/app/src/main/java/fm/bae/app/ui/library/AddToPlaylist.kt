package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
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
import fm.bae.app.data.PlaylistListingStore
import fm.bae.app.factsLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.retryAction

/**
 * Adding a track to a playlist from a track's menu: whether the menu offers it, and the track whose
 * sheet of playlists is open, if any. Only the sheet reads [trackId], so opening the sheet redraws
 * the sheet and not the page.
 */
internal class TrackPlaylistAdd(
    private val playlists: PlaylistListingStore,
) {
    /** Whether the menu offers Add to playlist: the person can make playlists. */
    val offered: Boolean get() = playlists.listing?.canCreate == true

    /** The track whose Add to playlist sheet is open. */
    var trackId by mutableStateOf<String?>(null)
        private set

    /** Open the sheet for [trackId]. */
    fun ask(trackId: String) {
        this.trackId = trackId
    }

    /** Close the sheet. */
    fun dismiss() {
        trackId = null
    }
}

/** The Add to playlist state of a page with tracks. The page reads the person's playlists while it is on screen. */
@Composable
internal fun rememberTrackPlaylistAdd(session: OpenLibrary): TrackPlaylistAdd {
    val playlists = rememberPlaylistListing(session)
    return remember(playlists) { TrackPlaylistAdd(playlists) }
}

/** The sheet of the person's playlists for the track [add] holds, with New playlist… first. */
@Composable
internal fun TrackPlaylistAddShown(
    session: OpenLibrary,
    add: TrackPlaylistAdd,
) {
    val trackId = add.trackId ?: return
    var creating by remember(trackId) { mutableStateOf(false) }
    AndroidSheet(onDismiss = add::dismiss, title = stringResource(R.string.playlist_add_title)) {
        Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
            AndroidRow(
                title = stringResource(R.string.playlist_new_row),
                action = true,
                leadingIcon = Icons.Filled.Add,
                onClick = { creating = true },
            )
            PlaylistTargets(session, add, trackId)
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

/** The playlists [add]'s track can go into, once their listing answers; its error, or a spinner until then. */
@Composable
private fun PlaylistTargets(
    session: OpenLibrary,
    add: TrackPlaylistAdd,
    trackId: String,
) {
    val context = LocalContext.current
    val playlists = session.browserPages.playlists
    val listing = playlists.listing
    val loadError = playlists.error
    when {
        listing != null -> {
            listing.addTargets.forEach { playlist ->
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

        loadError != null -> {
            AndroidNotice(
                error = loadError.error,
                modifier = Modifier.padding(horizontal = ThemeSpace.edge),
                action = loadError.error.retryAction(onClick = loadError.onRetry),
            )
        }

        // The listing has not answered yet, so a spinner shows in its place.
        else -> {
            AndroidSpinner(
                modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.group),
            )
        }
    }
}
