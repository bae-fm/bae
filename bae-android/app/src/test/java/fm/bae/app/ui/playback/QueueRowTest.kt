package fm.bae.app.ui.playback

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeTrackDisplay

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class QueueRowTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun compilationCreditsHaveSeparateLines() {
        val item =
            BridgeQueueEntry(
                entryId = "entry",
                trackId = "track",
                display =
                    BridgeTrackDisplay(
                        title = "Track Title",
                        artistNames = "Track Artist",
                        albumId = "album-1",
                        releaseId = "release-1",
                        albumTitle = "Compilation Album",
                        coverImage = null,
                    ),
                durationClock = null,
            )
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
                    QueueRow(item, Modifier, onClick = {}, onRemove = {})
                }
            }
        }
        val title = compose.onNodeWithText("Track Title", useUnmergedTree = true)
        val artist = compose.onNodeWithText("Track Artist", useUnmergedTree = true)
        val album = compose.onNodeWithText("Compilation Album", useUnmergedTree = true)
        title.assertIsDisplayed()
        artist.assertIsDisplayed()
        album.assertIsDisplayed()
        assertTrue(title.fetchSemanticsNode().boundsInRoot.bottom <= artist.fetchSemanticsNode().boundsInRoot.top)
        assertTrue(artist.fetchSemanticsNode().boundsInRoot.bottom <= album.fetchSemanticsNode().boundsInRoot.top)
    }
}
