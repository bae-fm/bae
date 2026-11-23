package fm.bae.app.ui.library

import android.app.Application
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.creditLineNamed
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeLibraryBrowseSection

// A phone's size, so the grid lays out two columns and composes every card.
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w411dp-h914dp")
class LibraryGridTest {
    @get:Rule
    val compose = createComposeRule()

    private val rows =
        listOf(
            BridgeAlbumAppearance(
                album = PreviewData.album(id = "alb-0", title = "Solo Album", year = 1971),
                primaryReleaseUnplayable = true,
                creditedWith = null,
            ),
            BridgeAlbumAppearance(
                album = PreviewData.album(id = "alb-1", title = "Shared Album", year = 1974),
                primaryReleaseUnplayable = false,
                creditedWith = creditLineNamed("Other Artist"),
            ),
            BridgeAlbumAppearance(
                album = PreviewData.album(id = "alb-2", title = "Undated Album", year = null),
                primaryReleaseUnplayable = false,
                creditedWith = creditLineNamed("Other Artist"),
            ),
        )

    private fun showGrid(
        layout: AlbumGridLayout,
        onSelectArtist: (String) -> Unit = {},
        onSelectAlbum: (String) -> Unit = {},
    ) {
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
                    AndroidAlbumGrid(
                        count = rows.size,
                        layout = layout,
                        rowAt = rows::getOrNull,
                        gridState = LazyGridState(),
                        onSelectAlbum = onSelectAlbum,
                        onSelectArtist = onSelectArtist,
                    )
                }
            }
        }
    }

    @Test
    fun aGroupOpensWithItsArtistAndItsCardsGiveTheYearAndOtherCredits() {
        var opened: String? = null
        showGrid(
            layout = AlbumGridLayout.Grouped(listOf(BridgeLibraryBrowseSection("artist-a", "Group Artist", null, 0uL, 3uL))),
            onSelectArtist = { opened = it },
        )

        compose.onNodeWithText("3 albums").assertExists()
        compose.onNodeWithText("1971").assertExists()
        compose.onNodeWithText("1974 · with Other Artist").assertExists()
        compose.onNodeWithText("with Other Artist").assertExists()
        compose.onNodeWithText("Group Artist").performClick()
        assertEquals("artist-a", opened)
    }

    @Test
    fun theFlatGridGivesEachAlbumsArtists() {
        showGrid(AlbumGridLayout.Flat)

        compose.onAllNodesWithText("Artist Name").assertCountEquals(rows.size)
        compose.onNodeWithText("1974 · with Other Artist").assertDoesNotExist()
    }

    @Test
    fun anUnplayableAlbumStillOpensItsPage() {
        var opened: String? = null
        showGrid(AlbumGridLayout.Flat, onSelectAlbum = { opened = it })

        compose.onNodeWithText("Solo Album").performClick()
        assertEquals("alb-0", opened)
    }
}
