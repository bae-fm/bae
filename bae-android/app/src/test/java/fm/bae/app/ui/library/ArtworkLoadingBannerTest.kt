package fm.bae.app.ui.library

import android.app.Application
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.data.ArtworkLoadingStore
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeEagerCacheFillProgress
import uniffi.bae_bridge.BridgeEagerCacheFillStatus

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class ArtworkLoadingBannerTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun failureDetailsAreHiddenUntilRequested() {
        val store = ArtworkLoadingStore {}
        store.apply(failure())
        compose.setContent { ArtworkLoadingBanner(store) }

        compose.onNodeWithText("The artwork object is missing.").assertDoesNotExist()
        compose.onNodeWithText("Details").performClick()
        compose.onNodeWithText("The artwork object is missing.").assertIsDisplayed()
        compose.onNodeWithText("Close").performClick()
        compose.onNodeWithText("The artwork object is missing.").assertDoesNotExist()
    }

    @Test
    fun dismissedFailureStaysHiddenUntilAnotherStatusArrives() {
        val store = ArtworkLoadingStore {}
        val failure = failure()
        store.apply(failure)
        compose.setContent { ArtworkLoadingBanner(store) }

        compose.onNodeWithContentDescription("Close").performClick()
        compose.runOnIdle { store.apply(failure) }
        compose.onNodeWithText("Some artwork couldn’t be downloaded.").assertDoesNotExist()
        compose.runOnIdle { store.apply(BridgeEagerCacheFillStatus.Scanning("core.artwork_cache.scanning")) }
        compose.runOnIdle { store.apply(failure) }
        compose.onNodeWithText("Some artwork couldn’t be downloaded.").assertIsDisplayed()
    }

    private fun failure() =
        BridgeEagerCacheFillStatus.Failed(
            titleKey = "core.artwork_cache.failed",
            progress = BridgeEagerCacheFillProgress(filesDone = 0uL, filesTotal = 2uL, bytesDone = 0uL, bytesTotal = 980_000uL),
            error = "The artwork object is missing.",
        )
}
