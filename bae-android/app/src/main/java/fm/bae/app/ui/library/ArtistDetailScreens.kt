package fm.bae.app.ui.library

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.appearance.ThemeSpace
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

@Preview(showBackground = true)
@Composable
private fun ArtistDetailContentPreview() {
    PreviewTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            ArtistDetailContent(
                detail = PreviewData.artistDetail(),
                onSelectAlbum = {},
            )
        }
    }
}
