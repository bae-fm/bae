package fm.bae.app.ui

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
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.formatDurationMs
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeTrackSearchResult

private const val DEBOUNCE_MS = 300L

private suspend fun fetchSearchResults(
    session: OpenLibrary,
    query: String,
): Pair<BridgeSearchResults, Map<String, String>> {
    delay(DEBOUNCE_MS)
    val res = session.library.search(query)
    // imagePathIfExists is a blocking FS read; resolve album thumbnails off-main
    // (the search call itself is already suspend).
    val covers =
        withContext(Dispatchers.IO) {
            res.albums
                .mapNotNull { album ->
                    session.library.imagePathIfExists(album.primaryReleaseId)?.let { album.id to it }
                }.toMap()
        }
    return res to covers
}

/**
 * Library search results for a non-blank query. Debounces typing, runs the
 * bridge search, and renders two sections — Albums then Tracks. Both row types
 * open the album's detail (tracks never play directly); the parent routes the
 * tap through the same selected-album navigation as a grid card. Iterates and
 * renders only — the search call lives in [OpenLibrary.library].
 */
@Composable
fun SearchResultsScreen(
    session: OpenLibrary,
    query: String,
    onSelectAlbum: (String) -> Unit,
) {
    var results by remember { mutableStateOf<BridgeSearchResults?>(null) }
    var coverPaths by remember { mutableStateOf<Map<String, String>>(emptyMap()) }
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
            val (res, covers) = fetchSearchResults(session, query)
            results = res
            coverPaths = covers
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            error = e.message ?: appContext.getString(R.string.search_failed)
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

            current != null && current.albums.isEmpty() && current.tracks.isEmpty() -> {
                Text(
                    text = stringResource(R.string.search_no_results, query),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
                )
            }

            current != null -> {
                SearchResultsList(
                    results = current,
                    coverPaths = coverPaths,
                    onSelectAlbum = onSelectAlbum,
                )
            }
        }
    }
}

@Composable
private fun SearchResultsList(
    results: BridgeSearchResults,
    coverPaths: Map<String, String>,
    onSelectAlbum: (String) -> Unit,
) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        if (results.albums.isNotEmpty()) {
            item { SectionHeader(stringResource(R.string.search_section_albums)) }
            items(results.albums, key = { "album:${it.id}" }) { album ->
                AlbumResultRow(
                    album = album,
                    coverPath = coverPaths[album.id],
                    onClick = { onSelectAlbum(album.id) },
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
    coverPath: String?,
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
            path = coverPath,
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
        val durationLabel = formatDurationMs(track.durationMs)
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
