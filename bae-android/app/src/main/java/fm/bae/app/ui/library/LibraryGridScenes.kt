package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.PreviewData
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeLibraryPageWindow

// Enough fixture albums to fill the screen below the scrolled scene's first row.
private const val SCENE_ALBUM_COUNT = 24

/** The fixture grid's rows: flat, or grouped by artist under [SCENE_SECTIONS]. */
internal enum class SceneGrid { FLAT, GROUPED }

/** The grouped fixture's artists, each group holding the next of their album counts. */
private val SCENE_SECTIONS: List<BridgeLibraryBrowseSection> =
    buildList {
        var offset = 0uL
        for ((name, albums) in listOf(
            "Artist Name" to 3uL,
            "Another Artist" to 2uL,
            "Third Artist" to 5uL,
            "Fourth Artist" to 4uL,
            "Fifth Artist" to 6uL,
        )) {
            add(BridgeLibraryBrowseSection("artist-$size", name, null, BridgeLibraryPageWindow(offset, albums)))
            offset += albums
        }
    }

/**
 * The library home screenshot scenes: the header over fixture albums with empty
 * cover tiles, in the app chrome, needing no session. The grid starts at its
 * item [firstItem] (an artist's header counts as one when grouped), and
 * [openMenu] shows that header menu open.
 */
@Composable
internal fun LibraryGridScene(
    grid: SceneGrid,
    firstItem: Int,
    openMenu: LibraryHeaderMenu?,
) {
    BaeAppChrome {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            val state =
                remember {
                    LibraryBrowserState().apply {
                        this.openMenu = openMenu
                        groupAlbumsByArtist = grid == SceneGrid.GROUPED
                    }
                }
            val gridState = rememberLazyGridState(initialFirstVisibleItemIndex = firstItem)
            val collapse = rememberHeadingCollapse(gridState)
            Column(modifier = Modifier.fillMaxSize()) {
                LibraryHeader(
                    state = state,
                    collapse = { collapse.value },
                    onShuffleLibrary = {},
                    onSettings = {},
                    syncActivity = {},
                )
                when (grid) {
                    SceneGrid.FLAT -> {
                        LibraryGridSceneAlbums(gridState)
                    }

                    SceneGrid.GROUPED -> {
                        LibraryGridBacking(
                            count = SCENE_SECTIONS.sumOf { it.window.limit.toInt() },
                            sections = SCENE_SECTIONS,
                            rowAt = ::sceneGroupedRow,
                            gridState = gridState,
                            onSelectAlbum = {},
                            onSelectArtist = {},
                        )
                    }
                }
            }
        }
    }
}

/**
 * The flat grid's fixture albums, scrolled by [gridState], under a
 * [LocalImageStore] the caller provides; the mini player's scenes show them
 * above the bar.
 */
@Composable
internal fun LibraryGridSceneAlbums(gridState: LazyGridState) {
    LibraryGridBacking(
        count = SCENE_ALBUM_COUNT,
        sections = emptyList(),
        rowAt = ::sceneFlatRow,
        gridState = gridState,
        onSelectAlbum = {},
        onSelectArtist = {},
    )
}

/**
 * A flat scene's row at [index]: the first title and the third album's artists
 * run past a card's width, to show them cut short with "…".
 */
private fun sceneFlatRow(index: Int): BridgeAlbumAppearance =
    BridgeAlbumAppearance(
        album =
            PreviewData.album(
                id = "alb-$index",
                title = if (index == 0) "A Long Album Title That Runs Past the Card" else "Album ${index + 1}",
                artistNames = if (index == 2) "Artist Name, Second Artist Name and Third Artist" else "Artist Name",
            ),
        creditedWith = null,
    )

/**
 * A grouped scene's row at [index]: every third album credits another artist
 * besides its group's, and the fifth states no year.
 */
private fun sceneGroupedRow(index: Int): BridgeAlbumAppearance =
    BridgeAlbumAppearance(
        album =
            PreviewData.album(
                id = "alb-$index",
                title = if (index == 0) "A Long Album Title That Runs Past the Card" else "Album ${index + 1}",
                year = if (index == 4) null else 1960 + 3 * index,
            ),
        creditedWith = if (index % 3 == 1) "Another Artist" else null,
    )

@Preview(showBackground = true)
@Composable
private fun LibraryGridScenePreview() {
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = null)
}

@Preview(showBackground = true)
@Composable
private fun LibraryGroupedScenePreview() {
    LibraryGridScene(grid = SceneGrid.GROUPED, firstItem = 0, openMenu = null)
}
