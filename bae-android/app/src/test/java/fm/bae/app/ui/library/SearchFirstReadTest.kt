package fm.bae.app.ui.library

import android.app.Application
import android.os.Looper
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.test.hasProgressBarRangeInfo
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.HANG_GUARD_MS
import fm.bae.app.data.LocalImageStore
import fm.bae.app.data.SEARCH_DEBOUNCE_MS
import fm.bae.app.openTestLibrary
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.BaeTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.time.Duration

/** Library search reads the local database, so nothing shows until its first answer, then the answer. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SearchFirstReadTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun nothingShowsUntilTheSearchAnswers() {
        val handle =
            FakeAppHandle(
                searchResults = { BridgeFixtures.searchResults(artists = listOf(BridgeFixtures.artistSummary(name = "Found Artist"))) },
                deliverSearchResultsImmediately = false,
            )
        val session = openTestLibrary(handle)
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalImageStore provides session.imageStore) {
                    SearchResultsScreen(
                        session = session,
                        query = "found",
                        onSelectAlbum = {},
                        onSelectArtist = {},
                        onSelectComposer = {},
                        onSelectWork = {},
                    )
                }
            }
        }
        compose.waitForIdle()
        compose.onNode(hasProgressBarRangeInfo(ProgressBarRangeInfo.Indeterminate)).assertDoesNotExist()
        compose.onNodeWithText("Found Artist").assertDoesNotExist()

        // The search moves to the typed text once typing pauses, and now answers it.
        handle.deliverSearchResultsImmediately = true
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(SEARCH_DEBOUNCE_MS))
        compose.waitUntil(HANG_GUARD_MS) { compose.onAllNodesWithText("Found Artist").fetchSemanticsNodes().isNotEmpty() }
        compose.onNode(hasProgressBarRangeInfo(ProgressBarRangeInfo.Indeterminate)).assertDoesNotExist()
    }
}
