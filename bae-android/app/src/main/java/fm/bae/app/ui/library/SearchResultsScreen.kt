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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.durationClockText
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.CoverBytesCache
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.LocalCoverBytesCache
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeTrackSearchResult
import uniffi.bae_bridge.BridgeWorkSummary

private const val DEBOUNCE_MS = 300L
private const val TAG = "bae.SearchResultsScreen"
private val logger = BaeLogger(TAG)

internal fun BridgeSearchResults.hasNoResults(): Boolean =
    albums.isEmpty() &&
        artists.isEmpty() &&
        tracks.isEmpty() &&
        composers.isEmpty() &&
        works.isEmpty()

private suspend fun fetchSearchResults(
    session: OpenLibrary,
    query: String,
): BridgeSearchResults {
    delay(DEBOUNCE_MS)
    // Each album carries its own cover reference; the rows fetch the bytes by id.
    return session.library.search(query)
}

/**
 * Library search results for a non-blank query. Debounces typing, runs the
 * bridge search, and renders one section per result kind — Albums, Artists,
 * Tracks, Composers, Works. Each row opens its subject's destination; the
 * parent routes the tap through the same navigation as the corresponding grid
 * card or list row. Iterates and renders only — the search call lives in
 * [OpenLibrary.library].
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
    var results by remember { mutableStateOf<BridgeSearchResults?>(null) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    val appContext = LocalContext.current

    // Re-keyed on query: a new keystroke cancels the in-flight debounce + search
    // (the suspend bridge call included), so only the latest query's results land.
    // `results` is not cleared between keystrokes, keeping prior results visible
    // while the next search runs instead of flashing the spinner.
    LaunchedEffect(query) {
        loading = true
        error = null
        try {
            results = fetchSearchResults(session, query)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Search failed", e)
            error = appContext.getString(R.string.search_failed)
        } finally {
            loading = false
        }
    }

    val current = results
    val currentError = error
    Box(modifier = Modifier.fillMaxSize()) {
        when {
            currentError != null -> {
                Text(
                    text = currentError,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
                )
            }

            loading && current == null -> {
                CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
            }

            current != null &&
                current.hasNoResults() -> {
                Text(
                    text = stringResource(R.string.search_no_results, query),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
                )
            }

            current != null -> {
                SearchResultsList(
                    results = current,
                    loadImage = session.library::imageBytes,
                    onSelectAlbum = onSelectAlbum,
                    onSelectArtist = onSelectArtist,
                    onSelectComposer = onSelectComposer,
                    onSelectWork = onSelectWork,
                )
            }
        }
    }
}

@Composable
private fun SearchResultsList(
    results: BridgeSearchResults,
    loadImage: suspend (imageId: String) -> ByteArray?,
    onSelectAlbum: (String) -> Unit,
    onSelectArtist: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectWork: (String) -> Unit,
) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        if (results.albums.isNotEmpty()) {
            item { SectionHeader(stringResource(R.string.search_section_albums)) }
            items(results.albums, key = { "album:${it.id}" }) { album ->
                AlbumResultRow(
                    album = album,
                    loadImage = loadImage,
                    onClick = { onSelectAlbum(album.id) },
                )
            }
        }
        if (results.artists.isNotEmpty()) {
            item { SectionHeader(stringResource(R.string.search_section_artists)) }
            items(results.artists, key = { "artist:${it.artistId}" }) { artist ->
                ArtistSummaryRow(
                    artist = artist,
                    loadImage = loadImage,
                    onClick = { onSelectArtist(artist.artistId) },
                )
            }
        }
        if (results.tracks.isNotEmpty()) {
            item { SectionHeader(stringResource(R.string.search_section_tracks)) }
            items(results.tracks, key = { "track:${it.id}" }) { track ->
                TrackResultRow(
                    track = track,
                    onClick = { onSelectAlbum(track.albumId) },
                )
            }
        }
        if (results.composers.isNotEmpty()) {
            item { SectionHeader(stringResource(R.string.search_section_composers)) }
            items(results.composers, key = { "composer:${it.artistId}" }) { composer ->
                ComposerResultRow(
                    composer = composer,
                    loadImage = loadImage,
                    onClick = { onSelectComposer(composer.artistId) },
                )
            }
        }
        if (results.works.isNotEmpty()) {
            item { SectionHeader(stringResource(R.string.search_section_works)) }
            items(results.works, key = { "work:${it.workId}" }) { work ->
                WorkResultRow(
                    work = work,
                    loadImage = loadImage,
                    onClick = { onSelectWork(work.workId) },
                )
            }
        }
    }
}

@Composable
private fun SectionHeader(title: String) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleSmall,
        fontWeight = FontWeight.Bold,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
    )
}

@Composable
private fun AlbumResultRow(
    album: BridgeAlbumSearchResult,
    loadImage: suspend (imageId: String) -> ByteArray?,
    onClick: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            coverId = album.cover?.id,
            coverVersion = album.cover?.version,
            loadImage = loadImage,
            cornerRadius = 4.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
            contentDescription = album.title,
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = album.title,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
            )
            Text(
                text = album.year?.let { "${album.artistName} · $it" } ?: album.artistName,
                style = MaterialTheme.typography.bodySmall,
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
                .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = track.title,
                style = MaterialTheme.typography.bodyLarge,
                maxLines = 1,
            )
            Text(
                text = "${track.artistName} — ${track.albumTitle}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
        val durationLabel = LocalContext.current.durationClockText(track.durationMs)
        if (durationLabel.isNotEmpty()) {
            Spacer(modifier = Modifier.width(12.dp))
            Text(
                text = durationLabel,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun ComposerResultRow(
    composer: BridgeComposerSummary,
    loadImage: suspend (imageId: String) -> ByteArray?,
    onClick: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            coverId = composer.image?.id,
            coverVersion = composer.image?.version,
            loadImage = loadImage,
            cornerRadius = 4.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
            contentDescription = composer.name,
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = composer.name,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
            )
            Text(
                text = stringResource(R.string.work_count, composer.workCount.toLong()),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun WorkResultRow(
    work: BridgeWorkSummary,
    loadImage: suspend (imageId: String) -> ByteArray?,
    onClick: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            coverId = work.representativeCover?.id,
            coverVersion = work.representativeCover?.version,
            loadImage = loadImage,
            cornerRadius = 4.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
            contentDescription = work.title,
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = work.title,
                style = MaterialTheme.typography.bodyLarge,
                maxLines = 1,
            )
            val composerNames = work.composerNames
            if (!composerNames.isNullOrBlank()) {
                Text(
                    text = composerNames,
                    style = MaterialTheme.typography.bodySmall,
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
        CompositionLocalProvider(LocalCoverBytesCache provides CoverBytesCache()) {
            SearchResultsList(
                results = PreviewData.searchResults(),
                loadImage = { _ -> null },
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
