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
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeLibraryPageWindow

// A phone's size, so the grid lays out two columns and composes every card.
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class, qualifiers = "w411dp-h914dp")
class LibraryGridTest {
    @get:Rule
    val compose = createComposeRule()

    private val rows =
        listOf(
            BridgeAlbumAppearance(PreviewData.album(id = "alb-0", title = "Solo Album", year = 1971), creditedWith = null),
            BridgeAlbumAppearance(PreviewData.album(id = "alb-1", title = "Shared Album", year = 1974), "Other Artist"),
            BridgeAlbumAppearance(PreviewData.album(id = "alb-2", title = "Undated Album", year = null), "Other Artist"),
        )

    private fun showGrid(
        sections: List<BridgeLibraryBrowseSection>,
        onSelectArtist: (String) -> Unit = {},
    ) {
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
                    LibraryGridBacking(
                        count = rows.size,
                        sections = sections,
                        rowAt = rows::getOrNull,
                        gridState = LazyGridState(),
                        onSelectAlbum = {},
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
            sections = listOf(BridgeLibraryBrowseSection("artist-a", "Group Artist", null, BridgeLibraryPageWindow(0uL, 3uL))),
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
        showGrid(sections = emptyList())

        compose.onAllNodesWithText("Artist Name").assertCountEquals(rows.size)
        compose.onNodeWithText("1974 · with Other Artist").assertDoesNotExist()
    }
}
