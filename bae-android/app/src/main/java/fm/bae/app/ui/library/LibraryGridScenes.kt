package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
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
 * item [firstItem] (an artist's header counts as one when grouped),
 * [openMenu] shows that header menu's contents open over the grid, and
 * [syncActivity] fills the top bar's place for the Activity button.
 */
@Composable
internal fun LibraryGridScene(
    grid: SceneGrid,
    firstItem: Int,
    openMenu: LibraryHeaderMenu?,
    syncActivity: @Composable () -> Unit,
) {
    BaeAppChrome {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            val state = remember { LibraryBrowserState().apply { groupAlbumsByArtist = grid == SceneGrid.GROUPED } }
            val gridState = rememberLazyGridState(initialFirstVisibleItemIndex = firstItem)
            val collapse = rememberHeadingCollapse(gridState)
            Column(modifier = Modifier.fillMaxSize()) {
                LibraryHeader(
                    state = state,
                    collapse = { collapse.value },
                    onShuffleLibrary = {},
                    onSettings = {},
                    syncActivity = syncActivity,
                )
                Box(modifier = Modifier.weight(1f)) {
                    when (grid) {
                        SceneGrid.FLAT -> LibraryGridSceneAlbums(gridState)
                        SceneGrid.GROUPED -> SceneGroupedAlbums(gridState)
                    }
                    if (openMenu != null) {
                        SceneMenu(menu = openMenu, state = state)
                    }
                }
            }
        }
    }
}

/**
 * A header menu's contents as they show open under the header: the views at
 * the heading's start, the ⋮ menu at the end. Drawn in a surface like the
 * menu's own, because layoutlib draws an open menu's popup blank.
 */
@Composable
private fun BoxScope.SceneMenu(
    menu: LibraryHeaderMenu,
    state: LibraryBrowserState,
) {
    Surface(
        shape = MenuDefaults.shape,
        color = MenuDefaults.containerColor,
        tonalElevation = MenuDefaults.TonalElevation,
        shadowElevation = MenuDefaults.ShadowElevation,
        modifier =
            Modifier
                .align(if (menu == LibraryHeaderMenu.VIEWS) Alignment.TopStart else Alignment.TopEnd)
                .padding(horizontal = ThemeSpace.related),
    ) {
        Column(modifier = Modifier.padding(vertical = ThemeSpace.related).width(IntrinsicSize.Max)) {
            when (menu) {
                LibraryHeaderMenu.VIEWS -> LibraryViewMenuItems(state)
                LibraryHeaderMenu.MORE -> LibraryMoreMenuItems(state = state, onShuffleLibrary = {}, onSettings = {})
            }
        }
    }
}

/** The grouped fixture's albums under their artists, scrolled by [gridState]. */
@Composable
private fun SceneGroupedAlbums(gridState: LazyGridState) {
    AndroidAlbumGrid(
        count = SCENE_SECTIONS.sumOf { it.window.limit.toInt() },
        sections = SCENE_SECTIONS,
        rowAt = ::sceneGroupedRow,
        gridState = gridState,
        onSelectAlbum = {},
        onSelectArtist = {},
    )
}

/**
 * The flat grid's fixture albums, scrolled by [gridState], under a
 * [LocalImageStore] the caller provides; the mini player's scenes show them
 * above the bar.
 */
@Composable
internal fun LibraryGridSceneAlbums(gridState: LazyGridState) {
    AndroidAlbumGrid(
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
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = null, syncActivity = {})
}

@Preview(showBackground = true)
@Composable
private fun LibraryGroupedScenePreview() {
    LibraryGridScene(grid = SceneGrid.GROUPED, firstItem = 0, openMenu = null, syncActivity = {})
}
