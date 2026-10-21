package fm.bae.app.ui.components

import android.app.Application
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.data.TestImages
import fm.bae.app.ui.BaeTheme
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLibraryImageType
import java.io.IOException

/**
 * A cover that exists but couldn't be read shows its own tile, one tap reads it
 * again, and only an album with no cover shows the no-cover glyph.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class CoverImageTest {
    @get:Rule
    val compose = createComposeRule()

    private val cover = BridgeImageRef(id = "rel-1", version = "1", imageType = BridgeLibraryImageType.COVER)

    @Test
    fun aFailedReadShowsTheFailedTileAndATapReadsAgain() {
        var reads = 0
        val store =
            store {
                reads += 1
                if (reads == 1) throw IOException("download failed")
                TestImages.png(64)
            }
        show(store, cover)

        compose.onNodeWithContentDescription(FAILED).assertIsDisplayed().performClick()
        compose.waitForIdle()

        assertEquals(2, reads)
        compose.onNodeWithContentDescription(COVER).assertIsDisplayed()
        compose.onNodeWithContentDescription(FAILED).assertDoesNotExist()
    }

    @Test
    fun bytesThatArentAnImageShowTheFailedTile() {
        show(store { "not an image".toByteArray() }, cover)

        compose.onNodeWithContentDescription(FAILED).assertIsDisplayed()
    }

    @Test
    fun anAlbumWithNoCoverShowsNoFailure() {
        show(store { error("an album with no cover reads nothing") }, cover = null)

        compose.onNodeWithContentDescription(FAILED).assertDoesNotExist()
    }

    private fun store(read: () -> ByteArray?) = ImageStore(fetchLibraryImageBytes = { read() }, dispatcher = Dispatchers.Unconfined)

    private fun show(
        store: ImageStore,
        cover: BridgeImageRef?,
    ) {
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalImageStore provides store) {
                    CoverImage(
                        cover = cover,
                        cornerRadius = 4.dp,
                        iconPadding = 8.dp,
                        modifier = Modifier.size(64.dp),
                        contentDescription = COVER,
                    )
                }
            }
        }
        compose.waitForIdle()
    }

    private companion object {
        const val COVER = "Album Title cover"
        const val FAILED = "Couldn't load image"
    }
}
