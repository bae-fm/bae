package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidTopBar

/** The playlists screen: every playlist this device holds, each opening its page, and New playlist. */
@Composable
internal fun PlaylistsScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
    backLabel: String,
    onSelectPlaylist: (String) -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize()) {
        AndroidTopBar(onBack = onBack, backLabel = backLabel, title = stringResource(R.string.library_mode_playlists))
        PlaylistListContent(
            session = session,
            page = rememberPlaylistListing(session),
            listState = rememberLazyListState(),
            onSelectPlaylist = onSelectPlaylist,
        )
    }
}
