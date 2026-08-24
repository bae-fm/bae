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
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.AlbumPageStore
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.CoverImage
import uniffi.bae_bridge.BridgeAlbum

// How many fixture albums the library-grid screenshot scene renders — enough to
// fill a couple of scrolled rows on a phone.
private const val SCENE_ALBUM_COUNT = 9

@Composable
internal fun LibraryGridContent(
    session: OpenLibrary,
    page: AlbumPageStore,
    gridState: LazyGridState,
    onSelectAlbum: (String) -> Unit,
) {
    LibraryPageContent(
        session = session,
        page = page,
        emptyMessage = stringResource(R.string.library_empty_syncing),
    ) {
        LibraryGridBacking(
            count = page.totalCount,
            albumAt = page.rows::get,
            gridState = gridState,
            onSelectAlbum = onSelectAlbum,
        )
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
    count: Int,
    albumAt: (Int) -> BridgeAlbum?,
    gridState: LazyGridState,
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
        items(count, key = { index -> albumAt(index)?.id ?: "album-slot-$index" }) { index ->
            val album = albumAt(index)
            if (album != null) {
                AlbumGridCard(album = album, onClick = { onSelectAlbum(album.id) })
            } else {
                Spacer(modifier = Modifier.fillMaxWidth().aspectRatio(1f))
            }
        }
    }
}

/**
 * The `library-grid` screenshot scene: a full grid of fixture albums in the app
 * chrome. The store resolves nothing and the preview renderer runs no load
 * effect, so every card holds its empty cover tile — deterministic, no session.
 */
@Composable
internal fun LibraryGridScene() {
    BaeAppChrome {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            LibraryGridBacking(
                count = SCENE_ALBUM_COUNT,
                albumAt = { i -> PreviewData.album(id = "alb-$i", title = "Album ${i + 1}") },
                gridState = rememberLazyGridState(),
                onSelectAlbum = {},
            )
        }
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
    onClick: () -> Unit,
) {
    Column(modifier = Modifier.clickable(onClick = onClick)) {
        CoverImage(
            cover = album.cover,
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
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            AlbumGridCard(album = PreviewData.album(), onClick = {})
        }
    }
}
