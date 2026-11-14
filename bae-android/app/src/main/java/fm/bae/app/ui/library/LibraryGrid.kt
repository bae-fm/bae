package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.creditLineNamed
import fm.bae.app.creditLineText
import fm.bae.app.data.AlbumPageStore
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.yearText
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeLibraryBrowseSection

@Composable
internal fun AndroidAlbumGridContainer(
    session: OpenLibrary,
    page: AlbumPageStore,
    gridState: LazyGridState,
    groupByArtist: Boolean,
    onSelectAlbum: (String) -> Unit,
    onSelectArtist: (String) -> Unit,
) {
    LibraryPageContent(
        session = session,
        page = page,
        emptyMessage = stringResource(R.string.library_empty_albums),
    ) {
        AndroidAlbumGrid(
            count = page.totalCount,
            sections = if (groupByArtist) page.sections else emptyList(),
            rowAt = page::rowAt,
            gridState = gridState,
            onSelectAlbum = onSelectAlbum,
            onSelectArtist = onSelectArtist,
        )
    }
}

/**
 * The album grid, taking its rows by index so the screenshot scenes can render
 * it without a live session. With [sections] it is grouped by artist: each
 * group opens with the artist's header, pinned at the top while its albums
 * scroll by.
 */
@Composable
internal fun AndroidAlbumGrid(
    count: Int,
    sections: List<BridgeLibraryBrowseSection>,
    rowAt: (Int) -> BridgeAlbumAppearance?,
    gridState: LazyGridState,
    onSelectAlbum: (String) -> Unit,
    onSelectArtist: (String) -> Unit,
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
            albumItems(0 until count, rowAt, grouped = false, onSelectAlbum)
        } else {
            sections.forEach { section ->
                stickyHeader(key = section.id, contentType = ARTIST_GROUP_HEADER) { index ->
                    val pinned by remember(gridState, index) {
                        derivedStateOf {
                            gridState.firstVisibleItemIndex > index ||
                                (gridState.firstVisibleItemIndex == index && gridState.firstVisibleItemScrollOffset > 0)
                        }
                    }
                    AndroidArtistHeading(
                        section = section,
                        pinned = pinned,
                        gridPadding = ThemeSpace.group,
                        onClick = { onSelectArtist(section.id) },
                    )
                }
                albumItems(
                    section.firstPosition.toInt() until (section.firstPosition + section.albumCount).toInt(),
                    rowAt,
                    grouped = true,
                    onSelectAlbum,
                )
            }
        }
    }
}

/** The content type of an artist's header among the grid's album cards. */
private const val ARTIST_GROUP_HEADER = "artist-group-header"

private fun LazyGridScope.albumItems(
    positions: IntRange,
    rowAt: (Int) -> BridgeAlbumAppearance?,
    grouped: Boolean,
    onSelectAlbum: (String) -> Unit,
) {
    items(positions.count(), key = { positions.first + it }) { local ->
        val index = positions.first + local
        val row = rowAt(index)
        if (row != null) {
            AndroidAlbumTile(
                title = row.album.title,
                line = if (grouped) groupedCardLine(row) else LocalContext.current.creditLineText(row.album.artistLine),
                cover = row.album.cover,
                onClick = { onSelectAlbum(row.album.id) },
            )
        } else {
            Spacer(modifier = Modifier.fillMaxWidth().aspectRatio(1f))
        }
    }
}

/** A grouped card's line under an album's title: the year, and the album's other credits when it has any. */
@Composable
private fun groupedCardLine(row: BridgeAlbumAppearance): String {
    val context = LocalContext.current
    val year = row.album.year?.let(context::yearText)
    val creditedWith = row.creditedWith?.let(context::creditLineText)
    return when {
        creditedWith == null -> year.orEmpty()
        year == null -> stringResource(R.string.album_credited_with, creditedWith)
        else -> stringResource(R.string.album_year_credited_with, year, creditedWith)
    }
}

@Preview(showBackground = true)
@Composable
private fun AlbumGridCardPreview() {
    PreviewTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            val row =
                BridgeAlbumAppearance(
                    album = PreviewData.album(),
                    creditedWith = creditLineNamed("Another Artist"),
                )
            AndroidAlbumTile(
                title = row.album.title,
                line = groupedCardLine(row),
                cover = row.album.cover,
                onClick = {},
            )
        }
    }
}
