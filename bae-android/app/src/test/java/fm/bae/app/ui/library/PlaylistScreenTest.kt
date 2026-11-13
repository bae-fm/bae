package fm.bae.app.ui.library

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.HANG_GUARD_MS
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.openTestLibrary
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgePlayOutcome

/** A playlist's page plays the playlist, or one of its entries, through the session. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class PlaylistScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private data class Played(
        val playlistId: String,
        val startPositionId: String?,
        val shuffle: Boolean,
    )

    private class PlayingHandle : FakeAppHandle() {
        val played = mutableListOf<Played>()

        override suspend fun playPlaylist(
            playlistId: String,
            startPositionId: String?,
            shuffle: Boolean,
        ): BridgePlayOutcome {
            played += Played(playlistId, startPositionId, shuffle)
            return BridgePlayOutcome.Playing
        }
    }

    private val handle = PlayingHandle()

    private fun showPage() {
        val session = openTestLibrary(handle)
        val detail =
            BridgeFixtures.playlistDetail(
                BridgeFixtures.playlistSummary(id = "playlist-1", name = "Late night"),
                entries =
                    listOf(
                        BridgeFixtures.playlistEntry("position-1", "Harbor Lights"),
                        BridgeFixtures.playlistEntry("position-2", "Copper Sky"),
                    ),
            )
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
                    PlaylistPage(session = session, detail = detail)
                }
            }
        }
    }

    @Test
    fun playStartsTheListFromItsFirstEntry() {
        showPage()
        compose.onNodeWithText("Play").performClick()
        compose.waitUntil(HANG_GUARD_MS) { handle.played.isNotEmpty() }

        assertEquals(listOf(Played("playlist-1", null, false)), handle.played)
    }

    @Test
    fun shuffleStartsTheListShuffled() {
        showPage()
        compose.onNodeWithText("Shuffle").performClick()
        compose.waitUntil(HANG_GUARD_MS) { handle.played.isNotEmpty() }

        assertEquals(listOf(Played("playlist-1", null, true)), handle.played)
    }

    @Test
    fun tappingAnEntryPlaysFromItInOrder() {
        showPage()
        compose.onNodeWithText("Copper Sky").performClick()
        compose.waitUntil(HANG_GUARD_MS) { handle.played.isNotEmpty() }

        assertEquals(listOf(Played("playlist-1", "position-2", false)), handle.played)
    }
}
