package fm.bae.app.ui.library

import android.app.Application
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeSortField

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidLibraryHeaderTest {
    @get:Rule
    val compose = createComposeRule()

    private fun showHeader(state: LibraryBrowserState) {
        compose.setContent {
            TestTheme {
                AndroidLibraryHeader(
                    state = state,
                    collapse = { 0f },
                    onShuffleLibrary = {},
                    syncActivity = {},
                    person = {},
                )
            }
        }
    }

    @Test
    fun theHeadingsMenuSwitchesViews() {
        val state = LibraryBrowserState()
        showHeader(state)

        compose.onNodeWithText("Albums").performClick()
        assertEquals(LibraryHeaderMenu.VIEWS, state.openMenu)
        compose.onNodeWithText("Artists").performClick()

        assertEquals(LibraryBrowserMode.ARTISTS, state.mode)
        assertNull(state.openMenu)
        compose.onNodeWithText("Artists").assertExists()
        compose.onNodeWithText("Albums").assertDoesNotExist()
    }

    @Test
    fun theMoreMenuSortsTheAlbumsAndGroupsThemByArtist() {
        val state = LibraryBrowserState()
        showHeader(state)

        compose.onNodeWithContentDescription("More options").performClick()
        compose.onNodeWithText("Sort by").assertExists()
        compose.onNodeWithText("Title").performClick()
        assertEquals(BridgeSortField.TITLE, state.sortCriterion.field)
        assertNull(state.openMenu)

        compose.onNodeWithContentDescription("More options").performClick()
        compose.onNodeWithText("Group by artist").performClick()
        assertTrue(state.groupAlbumsByArtist)
    }

    @Test
    fun theViewsAreAlbumsArtistsAndComposers() {
        val state = LibraryBrowserState()
        showHeader(state)

        compose.onNodeWithText("Albums").performClick()
        compose.onNodeWithText("Playlists").assertDoesNotExist()
        assertEquals(
            listOf(LibraryBrowserMode.ALBUMS, LibraryBrowserMode.ARTISTS, LibraryBrowserMode.COMPOSERS),
            LibraryBrowserMode.entries,
        )
    }

    @Test
    fun theMoreMenuShufflesTheLibraryAndHasNoSettings() {
        val state = LibraryBrowserState()
        var shuffled = false
        compose.setContent {
            TestTheme {
                AndroidLibraryHeader(
                    state = state,
                    collapse = { 0f },
                    onShuffleLibrary = { shuffled = true },
                    syncActivity = {},
                    person = {},
                )
            }
        }

        compose.onNodeWithContentDescription("More options").performClick()
        compose.onNodeWithText("Settings").assertDoesNotExist()
        compose.onNodeWithText("Shuffle library").performClick()
        assertTrue(shuffled)
    }

    @Test
    fun theHeadingFollowsTheScrollThroughTheFirstStretch() {
        assertEquals(0f, headingCollapse(firstIndex = 0, firstOffset = 0, distance = 100f, scrolling = true))
        assertEquals(0.3f, headingCollapse(firstIndex = 0, firstOffset = 30, distance = 100f, scrolling = true))
        assertEquals(1f, headingCollapse(firstIndex = 0, firstOffset = 140, distance = 100f, scrolling = true))
        assertEquals(1f, headingCollapse(firstIndex = 3, firstOffset = 0, distance = 100f, scrolling = true))
    }

    @Test
    fun aScrollThatStopsPartWayLeavesTheHeadingAtTheNearerSize() {
        assertEquals(0f, headingCollapse(firstIndex = 0, firstOffset = 30, distance = 100f, scrolling = false))
        assertEquals(1f, headingCollapse(firstIndex = 0, firstOffset = 70, distance = 100f, scrolling = false))
    }
}
