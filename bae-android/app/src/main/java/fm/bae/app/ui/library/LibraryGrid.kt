package fm.bae.app.ui.library

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.CoverBytesCache
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.LocalCoverBytesCache
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeAlbum

private const val PULL_REFRESH_SETTLE_MS = 900L

// How many fixture albums the library-grid screenshot scene renders — enough to
// fill a couple of scrolled rows on a phone.
private const val SCENE_ALBUM_COUNT = 9

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun LibraryGridContent(
    session: OpenLibrary,
    page: LibraryPage,
    gridState: LazyGridState,
    onSelectAlbum: (String) -> Unit,
) {
    var refreshing by remember { mutableStateOf(false) }
    val refreshScope = rememberCoroutineScope()
    val onRefresh: () -> Unit = {
        session.appHandle.triggerSync()
        refreshScope.launch {
            refreshing = true
            delay(PULL_REFRESH_SETTLE_MS)
            refreshing = false
        }
    }
    val pageError = page.error
    PullToRefreshBox(isRefreshing = refreshing, onRefresh = onRefresh, modifier = Modifier.fillMaxSize()) {
        when {
            pageError != null && page.order.isEmpty() -> {
                Column(
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(text = pageError.message, color = MaterialTheme.colorScheme.error)
                    TextButton(onClick = pageError.onRetry) { Text(stringResource(R.string.retry)) }
                }
            }

            page.loading && page.order.isEmpty() -> {
                CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
            }

            page.totalCount == 0 -> {
                Text(
                    text = stringResource(R.string.library_empty_syncing),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
                )
            }

            else -> {
                LibraryGridBacking(
                    albums = page.order.map { page.albums.getValue(it) },
                    gridState = gridState,
                    loadImage = session.library::imageBytes,
                    onSelectAlbum = onSelectAlbum,
                )
            }
        }
    }
}

/**
 * The album grid itself: adaptive columns of cover cards over a resolved album
 * list. Split out of [LibraryGridContent] so the `library-grid` screenshot scene
 * and the dev preview render the same grid the library shows, without a live
 * session — the caller supplies the albums, the cover-byte loader, and the
 * selection callback.
 */
@Composable
private fun LibraryGridBacking(
    albums: List<BridgeAlbum>,
    gridState: LazyGridState,
    loadImage: suspend (imageId: String) -> ByteArray?,
    onSelectAlbum: (String) -> Unit,
) {
    LazyVerticalGrid(
        columns = GridCells.Adaptive(minSize = 150.dp),
        state = gridState,
        contentPadding = PaddingValues(12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        items(albums, key = { it.id }) { album ->
            AlbumGridCard(
                album = album,
                loadImage = loadImage,
                onClick = { onSelectAlbum(album.id) },
            )
        }
    }
}

/**
 * The `library-grid` screenshot scene: a full grid of fixture albums in the app
 * chrome. Cover bytes never load (the loader returns null), so cards show the
 * placeholder cover icon — deterministic, no session.
 */
@Composable
internal fun LibraryGridScene() {
    BaeAppChrome {
        LibraryGridBacking(
            albums = List(SCENE_ALBUM_COUNT) { i -> PreviewData.album(id = "alb-$i", title = "Album ${i + 1}") },
            gridState = rememberLazyGridState(),
            loadImage = { null },
            onSelectAlbum = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun LibraryGridScenePreview() {
    LibraryGridScene()
}

@Composable
private fun AlbumGridCard(
    album: BridgeAlbum,
    loadImage: suspend (imageId: String) -> ByteArray?,
    onClick: () -> Unit,
) {
    Column(modifier = Modifier.clickable(onClick = onClick)) {
        CoverImage(
            coverId = album.cover?.id,
            coverVersion = album.cover?.version,
            loadImage = loadImage,
            cornerRadius = 6.dp,
            iconPadding = 40.dp,
            modifier = Modifier.fillMaxWidth().aspectRatio(1f),
            contentDescription = album.title,
        )
        Spacer(modifier = Modifier.height(6.dp))
        Text(
            text = album.title,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
        )
        Text(
            text = album.artistNames,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun AlbumGridCardPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalCoverBytesCache provides CoverBytesCache()) {
            AlbumGridCard(album = PreviewData.album(), loadImage = { _ -> null }, onClick = {})
        }
    }
}
