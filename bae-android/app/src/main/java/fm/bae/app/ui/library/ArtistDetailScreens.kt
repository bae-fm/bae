package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidEyebrow
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.playback.AndroidNowPlayingBarContainer
import fm.bae.app.yearText
import uniffi.bae_bridge.BridgeArtistDetail

@Composable
internal fun ArtistDetailScreen(
    session: OpenLibrary,
    artistId: String,
    onBack: () -> Unit,
    onSelectAlbum: (String) -> Unit,
) {
    val query by session.libraryQueries.artist.state
        .collectAsState()
    val detail = query.value
    DisposableEffect(artistId, session) {
        session.libraryQueries.artist.activate(artistId)
        onDispose {
            session.libraryQueries.artist.deactivate(artistId)
        }
    }
    val loadError =
        when {
            query.error != null -> stringResource(R.string.artist_detail_load_failed)
            query.delivered && detail == null -> stringResource(R.string.artist_detail_not_found)
            else -> null
        }
    AndroidDetailScreen(
        onBack = onBack,
        nowPlayingBar = { AndroidNowPlayingBarContainer(session = session) },
    ) {
        val loaded = detail
        val error = loadError
        when {
            error != null && loaded == null -> {
                ErrorText(
                    message = error,
                    modifier = Modifier.padding(ThemeSpace.page),
                )
            }

            // The detail is a local read; nothing shows until it lands.
            loaded == null -> {}

            else -> {
                error?.let {
                    ErrorText(
                        message = it,
                        modifier = Modifier.padding(ThemeSpace.related),
                    )
                }
                ArtistDetailContent(
                    detail = loaded,
                    onSelectAlbum = onSelectAlbum,
                )
            }
        }
    }
}

@Composable
private fun ArtistDetailContent(
    detail: BridgeArtistDetail,
    onSelectAlbum: (String) -> Unit,
) {
    val context = LocalContext.current
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item {
            AndroidLibraryRow(
                title = detail.artist.name,
                subtitle = context.coreString(detail.artist.countLine),
                art = detail.artist.image,
                missing = detail.artist.image == null,
            )
        }
        if (detail.creditNames.isNotEmpty()) {
            item {
                Column(
                    modifier =
                        Modifier.padding(start = ThemeSpace.edge, end = ThemeSpace.edge, bottom = ThemeSpace.related),
                ) {
                    AndroidEyebrow(
                        text = stringResource(R.string.artist_also_credited_as),
                        modifier = Modifier.padding(bottom = ThemeSpace.inline),
                    )
                    Text(
                        text = detail.creditNames.joinToString(" · ") { it.credit.name },
                        style = ThemeText.body.style,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
        if (detail.albums.isNotEmpty()) {
            item { AndroidSearchSection(stringResource(R.string.search_section_albums)) }
            items(detail.albums, key = { it.id }) { album ->
                AndroidLibraryRow(
                    title = album.title,
                    subtitle = album.year?.let(context::yearText),
                    art = album.cover,
                    inset = true,
                    onClick = { onSelectAlbum(album.id) },
                )
            }
        }
    }
}
