package fm.bae.app.ui.albumdetail

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.TestTheme
import kotlinx.coroutines.awaitCancellation
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/** A gallery image can come from the release's storage, so while it loads the gallery says so. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class GalleryLoadingTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun anImageStillLoadingIsNamed() {
        // Bytes that never arrive keep the gallery on its load.
        val store = ImageStore(fetchLibraryImageBytes = { awaitCancellation() }, fetchReleaseImageBytes = { _, _ -> awaitCancellation() })
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalImageStore provides store, LocalBugReports provides BugReports()) {
                    GalleryDialog(releaseId = "rel-1", items = listOf(PreviewData.galleryItem()), onDismiss = {})
                }
            }
        }

        compose.onNodeWithText("Loading the image…").assertIsDisplayed()
    }
}
