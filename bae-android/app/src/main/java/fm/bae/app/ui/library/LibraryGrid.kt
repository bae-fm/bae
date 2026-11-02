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
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
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
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CoverImage
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeLibraryBrowseSection

// Enough fixture albums to fill the screen below the scrolled scene's first row.
private const val SCENE_ALBUM_COUNT = 24

@Composable
internal fun LibraryGridContent(
    session: OpenLibrary,
    page: AlbumPageStore,
    gridState: LazyGridState,
    groupByArtist: Boolean,
    onSelectAlbum: (String) -> Unit,
) {
    LibraryPageContent(
        session = session,
        page = page,
        emptyMessage = stringResource(R.string.library_empty_albums),
    ) {
        LibraryGridBacking(
            count = page.totalCount,
            sections = if (groupByArtist) page.sections else emptyList(),
            albumAt = page.rows::get,
            gridState = gridState,
            onSelectAlbum = onSelectAlbum,
        )
    }
}

/**
 * The album grid, taking albums by index so the screenshot scene can render it
 * without a live session.
 */
@Composable
private fun LibraryGridBacking(
    count: Int,
    sections: List<BridgeLibraryBrowseSection> = emptyList(),
    albumAt: (Int) -> BridgeAlbum?,
    gridState: LazyGridState,
    onSelectAlbum: (String) -> Unit,
) {
    LazyVerticalGrid(
        columns = GridCells.Adaptive(minSize = 150.dp),
        state = gridState,
        contentPadding = PaddingValues(ThemeSpace.group),
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.group),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.group),
        modifier = Modifier.fillMaxSize(),
    ) {
        if (sections.isEmpty()) {
            albumItems(0 until count, albumAt, onSelectAlbum)
        } else {
            sections.forEach { section ->
                item(key = section.id, span = { GridItemSpan(maxLineSpan) }) {
                    Text(
                        text = section.title,
                        style = ThemeText.rowTitle.style,
                        modifier = Modifier.semantics { heading() },
                    )
                }
                albumItems(
                    section.window.offset.toInt() until (section.window.offset + section.window.limit).toInt(),
                    albumAt,
                    onSelectAlbum,
                )
            }
        }
    }
}

private fun LazyGridScope.albumItems(
    positions: IntRange,
    albumAt: (Int) -> BridgeAlbum?,
    onSelectAlbum: (String) -> Unit,
) {
    items(positions.count(), key = { positions.first + it }) { local ->
        val index = positions.first + local
        val album = albumAt(index)
        if (album != null) {
            AlbumGridCard(album = album, onClick = { onSelectAlbum(album.id) })
        } else {
            Spacer(modifier = Modifier.fillMaxWidth().aspectRatio(1f))
        }
    }
}

/**
 * The library home screenshot scenes: the header over fixture albums with empty
 * cover tiles, in the app chrome, needing no session. The grid starts at
 * [firstAlbum], and [openMenu] shows that header menu open.
 */
@Composable
internal fun LibraryGridScene(
    firstAlbum: Int,
    openMenu: LibraryHeaderMenu?,
) {
    BaeAppChrome {
        val state = remember { LibraryBrowserState().apply { this.openMenu = openMenu } }
        val gridState = rememberLazyGridState(initialFirstVisibleItemIndex = firstAlbum)
        val collapse = rememberHeadingCollapse(gridState)
        Column(modifier = Modifier.fillMaxSize()) {
            LibraryHeader(
                state = state,
                collapse = { collapse.value },
                onShuffleLibrary = {},
                onSettings = {},
                syncActivity = {},
            )
            LibraryGridSceneAlbums(gridState)
        }
    }
}

/**
 * The library scenes' albums, with no cover resolved, scrolled by [gridState];
 * the mini player's scenes show them above the bar.
 */
@Composable
internal fun LibraryGridSceneAlbums(gridState: LazyGridState) {
    CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
        LibraryGridBacking(
            count = SCENE_ALBUM_COUNT,
            albumAt = ::sceneAlbum,
            gridState = gridState,
            onSelectAlbum = {},
        )
    }
}

/**
 * A scene's album at [index]: the first title and the third album's artists run
 * past a card's width, to show them cut short with "…".
 */
private fun sceneAlbum(index: Int): BridgeAlbum =
    PreviewData.album(
        id = "alb-$index",
        title = if (index == 0) "A Long Album Title That Runs Past the Card" else "Album ${index + 1}",
        artistNames = if (index == 2) "Artist Name, Second Artist Name and Third Artist" else "Artist Name",
    )

@Preview(showBackground = true)
@Composable
private fun LibraryGridScenePreview() {
    LibraryGridScene(firstAlbum = 0, openMenu = null)
}

@Composable
private fun AlbumGridCard(
    album: BridgeAlbum,
    onClick: () -> Unit,
) {
    Column(modifier = Modifier.clickable(onClick = onClick)) {
        CoverImage(
            cover = album.cover,
            cornerRadius = ThemeRadius.cover,
            iconPadding = 40.dp,
            modifier = Modifier.fillMaxWidth().aspectRatio(1f),
            contentDescription = album.title,
        )
        Spacer(modifier = Modifier.height(ThemeSpace.compact))
        Text(
            text = album.title,
            style = ThemeText.rowTitle.style,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            text = album.artistNames,
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
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
