package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.creditLineText
import fm.bae.app.data.LiveQueryState
import fm.bae.app.displayError
import fm.bae.app.durationClockLabel
import fm.bae.app.factsLine
import fm.bae.app.namesLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.retryAction
import fm.bae.app.yearText
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeTrackSearchResult

internal fun BridgeSearchResults.hasNoResults(): Boolean = !found

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

    LaunchedEffect(query, session) {
        session.libraryQueries.search.activate(query)
    }
    DisposableEffect(session) {
        onDispose {
            session.libraryQueries.search.deactivate()
        }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        when (val answer = state) {
            is LiveQueryState.Failed -> {
                val shown = LocalContext.current.displayError(answer.error)
                AndroidNotice(
                    title = stringResource(R.string.search_failed),
                    advice = null,
                    error = shown,
                    look = NoticeLook.CENTERED,
                    action = shown.retryAction(onClick = session.libraryQueries.search::retry),
                )
            }

            // Library search is a local read; nothing shows until it lands.
            LiveQueryState.Waiting -> {}

            is LiveQueryState.Answered -> {
                val results = answer.value
                if (results.hasNoResults()) {
                    Text(
                        text = stringResource(R.string.search_no_results, query),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                    )
                } else {
                    SearchResultsList(
                        results = results,
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
    val context = LocalContext.current
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        if (results.sectionCounts.albums > 0u) {
            item { AndroidSearchSection(stringResource(R.string.search_section_albums)) }
            items(results.albums, key = { "album:${it.id}" }) { album ->
                AndroidLibraryRow(
                    title = album.title,
                    subtitle =
                        context.factsLine(
                            listOf(context.creditLineText(album.artistLine), album.year?.let(context::yearText)),
                        ),
                    art = album.cover,
                    onClick = { onSelectAlbum(album.id) },
                )
            }
        }
        if (results.sectionCounts.artists > 0u) {
            item { AndroidSearchSection(stringResource(R.string.search_section_artists)) }
            items(results.artists, key = { "artist:${it.artistId}" }) { artist ->
                AndroidLibraryRow(
                    title = artist.name,
                    subtitle = context.coreString(artist.countLine),
                    art = artist.image,
                    missing = artist.image == null,
                    onClick = { onSelectArtist(artist.artistId) },
                )
            }
        }
        if (results.sectionCounts.tracks > 0u) trackResults(results.tracks, onSelectAlbum)
        if (results.sectionCounts.composers > 0u) {
            item { AndroidSearchSection(stringResource(R.string.search_section_composers)) }
            items(results.composers, key = { "composer:${it.artistId}" }) { composer ->
                AndroidLibraryRow(
                    title = composer.name,
                    subtitle = context.coreString(composer.countLine),
                    art = composer.image,
                    missing = composer.image == null,
                    onClick = { onSelectComposer(composer.artistId) },
                )
            }
        }
        if (results.sectionCounts.works > 0u) {
            item { AndroidSearchSection(stringResource(R.string.search_section_works)) }
            items(results.works, key = { "work:${it.workId}" }) { work ->
                AndroidLibraryRow(
                    title = work.title,
                    subtitle = work.composerNames,
                    art = work.representativeCover,
                    onClick = { onSelectWork(work.workId) },
                )
            }
        }
    }
}

private fun LazyListScope.trackResults(
    tracks: List<BridgeTrackSearchResult>,
    onSelectAlbum: (String) -> Unit,
) {
    item { AndroidSearchSection(stringResource(R.string.search_section_tracks)) }
    items(tracks, key = { "track:${it.id}" }) { track ->
        AndroidLibraryRow(
            title = track.title,
            subtitle =
                LocalContext.current.let {
                    it.namesLine(listOf(it.creditLineText(track.artistLine), track.albumTitle))
                },
            trailing = LocalContext.current.durationClockLabel(track.durationClock),
            noArt = true,
            onClick = { onSelectAlbum(track.albumId) },
        )
    }
}
