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
import fm.bae.app.data.LiveQueryState
import fm.bae.app.factSeparator
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidErrorLine
import fm.bae.app.ui.components.AndroidEyebrow
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.playback.AndroidNowPlayingBarContainer
import fm.bae.app.yearText
import uniffi.bae_bridge.BridgeArtistDetail

@Composable
internal fun ArtistDetailScreen(
    session: OpenLibrary,
    artistId: String,
    onBack: () -> Unit,
    backLabel: String,
    onSelectAlbum: (String) -> Unit,
) {
    val query by session.libraryQueries.artist.state
        .collectAsState()
    DisposableEffect(artistId, session) {
        session.libraryQueries.artist.activate(artistId)
        onDispose {
            session.libraryQueries.artist.deactivate(artistId)
        }
    }
    AndroidDetailScreen(
        onBack = onBack,
        backLabel = backLabel,
        nowPlayingBar = { AndroidNowPlayingBarContainer(session = session) },
    ) {
        when (val answer = query) {
            is LiveQueryState.Failed -> {
                AndroidErrorLine(
                    message = stringResource(R.string.artist_detail_load_failed),
                    modifier = Modifier.padding(ThemeSpace.page),
                )
            }

            // The detail is a local read; nothing shows until it lands.
            LiveQueryState.Waiting -> {}

            is LiveQueryState.Answered -> {
                val detail = answer.value
                if (detail == null) {
                    AndroidErrorLine(
                        message = stringResource(R.string.artist_detail_not_found),
                        modifier = Modifier.padding(ThemeSpace.page),
                    )
                } else {
                    ArtistDetailContent(
                        detail = detail,
                        onSelectAlbum = onSelectAlbum,
                    )
                }
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
        if (detail.creditNameCount > 0u) {
            item {
                AndroidArtistOtherNames(
                    names = detail.creditNames.map { it.credit.name },
                    modifier =
                        Modifier.padding(start = ThemeSpace.edge, end = ThemeSpace.edge, bottom = ThemeSpace.related),
                )
            }
        }
        if (detail.albumCount > 0u) {
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

/** The other names the artist's credits print, under the Also credited as heading. */
@Composable
internal fun AndroidArtistOtherNames(
    names: List<String>,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier) {
        AndroidEyebrow(
            text = stringResource(R.string.artist_also_credited_as),
            modifier = Modifier.padding(bottom = ThemeSpace.inline),
        )
        Text(
            text = names.joinToString(LocalContext.current.factSeparator()),
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
