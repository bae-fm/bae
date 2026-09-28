package fm.bae.app.ui.library

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.durationClockLabel
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.ErrorText
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeTrackSearchResult
import uniffi.bae_bridge.BridgeWorkSummary

internal fun BridgeSearchResults.hasNoResults(): Boolean =
    albums.isEmpty() &&
        artists.isEmpty() &&
        tracks.isEmpty() &&
        composers.isEmpty() &&
        works.isEmpty()

/**
 * Library search results for a non-blank query, one section per result kind.
 */
@Composable
fun SearchResultsScreen(
    session: OpenLibrary,
    query: String,
    onSelectAlbum: (String) -> Unit,
    onSelectArtist: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectWork: (String) -> Unit,
) {
    val state by session.libraryQueries.search.state
        .collectAsState()
    val appContext = LocalContext.current

    LaunchedEffect(query, session) {
        session.libraryQueries.search.activate(query)
    }
    DisposableEffect(session) {
        onDispose {
            session.libraryQueries.search.deactivate()
        }
    }

    val current = state.value
    val currentError = state.error?.let { appContext.getString(R.string.search_failed) }
    Box(modifier = Modifier.fillMaxSize()) {
        when {
            currentError != null && current == null -> {
                ErrorText(
                    message = currentError,
                    modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                )
            }

            !state.delivered && current == null -> {
                CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
            }

            current != null &&
                current.hasNoResults() -> {
                Text(
                    text = stringResource(R.string.search_no_results, query),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                )
            }

            current != null -> {
                Column {
                    currentError?.let {
                        ErrorText(
                            message = it,
                            modifier = Modifier.padding(ThemeSpace.related),
                        )
                    }
                    SearchResultsList(
                        results = current,
                        onSelectAlbum = onSelectAlbum,
                        onSelectArtist = onSelectArtist,
                        onSelectComposer = onSelectComposer,
                        onSelectWork = onSelectWork,
                    )
                }
            }
        }
    }
}

@Composable
private fun SearchResultsList(
    results: BridgeSearchResults,
    onSelectAlbum: (String) -> Unit,
    onSelectArtist: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectWork: (String) -> Unit,
) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        if (results.albums.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_albums)) }
            items(results.albums, key = { "album:${it.id}" }) { album ->
                AlbumResultRow(
                    album = album,
                    onClick = { onSelectAlbum(album.id) },
                )
            }
        }
        if (results.artists.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_artists)) }
            items(results.artists, key = { "artist:${it.artistId}" }) { artist ->
                ArtistSummaryRow(
                    artist = artist,
                    onClick = { onSelectArtist(artist.artistId) },
                )
            }
        }
        if (results.tracks.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_tracks)) }
            items(results.tracks, key = { "track:${it.id}" }) { track ->
                TrackResultRow(
                    track = track,
                    onClick = { onSelectAlbum(track.albumId) },
                )
            }
        }
        if (results.composers.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_composers)) }
            items(results.composers, key = { "composer:${it.artistId}" }) { composer ->
                ComposerResultRow(
                    composer = composer,
                    onClick = { onSelectComposer(composer.artistId) },
                )
            }
        }
        if (results.works.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_works)) }
            items(results.works, key = { "work:${it.workId}" }) { work ->
                WorkResultRow(
                    work = work,
                    onClick = { onSelectWork(work.workId) },
                )
            }
        }
    }
}

@Composable
private fun AlbumResultRow(
    album: BridgeAlbumSearchResult,
    onClick: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = album.cover,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = Modifier.size(ThemeSize.rowArtwork),
            contentDescription = album.title,
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = album.title,
                style = ThemeText.rowTitle.style,
                maxLines = 1,
            )
            Text(
                text = album.year?.let { "${album.artistName} · $it" } ?: album.artistName,
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun TrackResultRow(
    track: BridgeTrackSearchResult,
    onClick: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = track.title,
                style = ThemeText.rowTitle.style,
                maxLines = 1,
            )
            Text(
                text = "${track.artistName}, ${track.albumTitle}",
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
        val durationLabel = LocalContext.current.durationClockLabel(track.durationClock)
        if (durationLabel.isNotEmpty()) {
            Spacer(modifier = Modifier.width(ThemeSpace.group))
            Text(
                text = durationLabel,
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun ComposerResultRow(
    composer: BridgeComposerSummary,
    onClick: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = composer.image,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = Modifier.size(ThemeSize.rowArtwork),
            contentDescription = composer.name,
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = composer.name,
                style = ThemeText.rowTitle.style,
                maxLines = 1,
            )
            Text(
                text = stringResource(R.string.work_count, composer.workCount.toLong()),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun WorkResultRow(
    work: BridgeWorkSummary,
    onClick: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = work.representativeCover,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = Modifier.size(ThemeSize.rowArtwork),
            contentDescription = work.title,
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = work.title,
                style = ThemeText.rowTitle.style,
                maxLines = 1,
            )
            val composerNames = work.composerNames
            if (!composerNames.isNullOrBlank()) {
                Text(
                    text = composerNames,
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun SearchResultsListPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            SearchResultsList(
                results = PreviewData.searchResults(),
                onSelectAlbum = {},
                onSelectArtist = {},
                onSelectComposer = {},
                onSelectWork = {},
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun TrackResultRowPreview() {
    BaeTheme {
        TrackResultRow(track = PreviewData.trackSearchResult(), onClick = {})
    }
}
