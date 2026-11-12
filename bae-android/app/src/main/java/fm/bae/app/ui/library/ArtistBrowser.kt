package fm.bae.app.ui.library

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ArtistPageStore
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.components.AndroidLibraryRow
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeArtistSortCriterion

@Composable
internal fun rememberArtistPage(
    session: OpenLibrary,
    sortCriterion: BridgeArtistSortCriterion,
): ArtistPageStore {
    val page = session.browserPages.artists
    DisposableEffect(page, sortCriterion) {
        page.activate(sortCriterion)
        onDispose(page::deactivate)
    }
    return page
}

@Composable
internal fun ArtistListContent(
    session: OpenLibrary,
    page: ArtistPageStore,
    listState: LazyListState,
    onSelectArtist: (String) -> Unit,
) {
    LibraryPageContent(
        session = session,
        page = page,
        emptyMessage = stringResource(R.string.library_empty_artists),
    ) {
        LaunchedEffect(page, listState) {
            snapshotFlow {
                val visible = listState.layoutInfo.visibleItemsInfo
                (visible.firstOrNull()?.index ?: 0) to (visible.lastOrNull()?.index ?: 0)
            }.distinctUntilChanged().collect { (first, last) ->
                page.reportVisibleRange(first, last)
            }
        }
        val context = LocalContext.current
        LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
            items(page.totalCount, key = { index -> page.rows[index]?.artistId ?: "artist-slot-$index" }) { index ->
                page.rows[index]?.let { artist ->
                    AndroidLibraryRow(
                        title = artist.name,
                        subtitle = context.coreString(artist.countLine),
                        art = artist.image,
                        missing = artist.image == null,
                        onClick = { onSelectArtist(artist.artistId) },
                    )
                }
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun ArtistListRowPreview() {
    PreviewTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            val artist = PreviewData.artistSummary()
            AndroidLibraryRow(
                title = artist.name,
                subtitle = LocalContext.current.coreString(artist.countLine),
                art = artist.image,
                missing = artist.image == null,
            )
        }
    }
}
