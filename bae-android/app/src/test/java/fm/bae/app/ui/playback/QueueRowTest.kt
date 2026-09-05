package fm.bae.app.ui.playback

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.playback.QueueItem
import fm.bae.app.ui.BaeTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class QueueRowTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun compilationCreditsHaveSeparateLines() {
        val item =
            QueueItem(
                entryId = "entry",
                trackId = "track",
                title = "Track Title",
                artist = "Track Artist",
                albumTitle = "Compilation Album",
                durationClock = null,
                coverImage = null,
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
