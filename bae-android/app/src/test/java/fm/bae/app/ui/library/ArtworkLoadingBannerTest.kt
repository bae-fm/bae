package fm.bae.app.ui.library

import android.app.Application
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.data.ArtworkLoadingStore
import fm.bae.app.ui.BaeTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeEagerCacheFillProgress
import uniffi.bae_bridge.BridgeEagerCacheFillStatus

private const val CANCELLED_TITLE = "Artwork download stopped."

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class ArtworkLoadingBannerTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun downloadingShowsItsProgressAndCancels() {
        var cancelled = false
        val store = ArtworkLoadingStore { cancelled = true }
        store.apply(BridgeEagerCacheFillStatus.Downloading(progress(bytesCached = 490_000uL)))
        compose.setContent { BaeTheme { ArtworkLoadingBanner(store) } }

        compose.onNodeWithText("Downloading artwork…").assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        compose.runOnIdle { assert(cancelled) }
    }

    @Test
    fun dismissedCancellationStaysHiddenUntilAnotherStatusArrives() {
        val store = ArtworkLoadingStore {}
        val cancelled = BridgeEagerCacheFillStatus.Cancelled(progress(bytesCached = 0uL))
        store.apply(cancelled)
        compose.setContent { BaeTheme { ArtworkLoadingBanner(store) } }

        compose.onNodeWithContentDescription("Close").performClick()
        compose.runOnIdle { store.apply(cancelled) }
        compose.onNodeWithText(CANCELLED_TITLE).assertDoesNotExist()
        compose.runOnIdle { store.apply(BridgeEagerCacheFillStatus.Downloading(progress(bytesCached = 1uL))) }
        compose.runOnIdle { store.apply(cancelled) }
        compose.onNodeWithText(CANCELLED_TITLE).assertIsDisplayed()
    }

    private fun progress(bytesCached: ULong) =
        BridgeEagerCacheFillProgress(
            filesCompleted = 0uL,
            filesTotal = 2uL,
            bytesCached = bytesCached,
            bytesTotal = 980_000uL,
        )
}
