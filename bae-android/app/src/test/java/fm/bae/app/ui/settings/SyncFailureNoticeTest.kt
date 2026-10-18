package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.data.SyncFailure
import fm.bae.app.ui.BaeTheme
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
                    error = SyncFailure("Update the app to continue syncing.", canReconnect = false),
                    onReconnect = {},
                )
            }
        }

        compose.onNodeWithText("Update the app to continue syncing.").assertIsDisplayed()
        compose.onNodeWithText("Reconnect").assertDoesNotExist()
    }

    @Test
    fun syncedWithoutAFailureShowsNothing() {
        compose.setContent {
            BaeTheme { SyncFailureNotice(BridgeSyncIndicator.Synced, error = null, onReconnect = {}) }
        }

        compose.onNodeWithText("Reconnect").assertDoesNotExist()
    }
}
