package fm.bae.app.ui.settings

import android.app.Application
import android.content.ClipboardManager
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import fm.bae.app.DisplayError
import fm.bae.app.data.SyncFailure
import fm.bae.app.ui.BaeTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeSyncIndicator

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SyncFailureNoticeTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun notConnectedOffersReconnecting() {
        compose.setContent {
            BaeTheme { SyncFailureNotice(BridgeSyncIndicator.Unconnected, error = null, onReconnect = {}) }
        }

        compose.onNodeWithText("Reconnect").assertIsDisplayed()
    }

    @Test
    fun aFailureReconnectingCantFixShowsItsReasonAlone() {
        compose.setContent {
            BaeTheme {
                SyncFailureNotice(
                    BridgeSyncIndicator.Synced,
                    error =
                        SyncFailure(
                            DisplayError("Update the app to continue syncing."),
                            canReconnect = false,
                            offersDeviceReset = false,
                        ),
                    onReconnect = {},
                )
            }
        }

        compose.onNodeWithText("Update the app to continue syncing.").assertIsDisplayed()
        compose.onNodeWithText("Reconnect").assertDoesNotExist()
    }

    /** A failure core reported shows its fault under the line, and Copy details copies the whole diagnostic. */
    @Test
    fun aFailureShowsItsCauseAndCopiesTheWholeDiagnostic() {
        val detail = "Network: S3 request failed: dispatch failure: io error: dns error: no such host\nsecond line"
        compose.setContent {
            BaeTheme {
                SyncFailureNotice(
                    BridgeSyncIndicator.Synced,
                    error =
                        SyncFailure(
                            DisplayError("Couldn't reach the network.", detail),
                            canReconnect = false,
                            offersDeviceReset = false,
                        ),
                    onReconnect = {},
                )
            }
        }

        compose.onNodeWithText("Couldn't reach the network.").assertIsDisplayed()
        compose
            .onNodeWithText("Network: S3 request failed: dispatch failure: io error: dns error: no such host")
            .assertIsDisplayed()
        compose.onNodeWithContentDescription("Copy details").performClick()
        val clipboard = ApplicationProvider.getApplicationContext<Application>().getSystemService(ClipboardManager::class.java)
        assertEquals(
            detail,
            clipboard.primaryClip
                ?.getItemAt(0)
                ?.text
                ?.toString(),
        )
    }

    @Test
    fun syncedWithoutAFailureShowsNothing() {
        compose.setContent {
            BaeTheme { SyncFailureNotice(BridgeSyncIndicator.Synced, error = null, onReconnect = {}) }
        }

        compose.onNodeWithText("Reconnect").assertDoesNotExist()
    }
}
