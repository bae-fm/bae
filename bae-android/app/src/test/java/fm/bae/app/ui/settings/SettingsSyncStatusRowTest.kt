package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.data.SyncFailure
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeSyncIndicator

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SettingsSyncStatusRowTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun appUpdateShowsTheReasonWithoutOfferingReconnection() {
        val error = mutableStateOf(SyncFailure("Update the app to continue syncing.", false))
        compose.setContent {
            SettingsSyncStatusRow(BridgeSyncIndicator.Error, error.value, {})
        }
        compose.onNodeWithText("Update the app to continue syncing.").assertIsDisplayed()
        compose.onNodeWithText("Reconnect").assertDoesNotExist()
        compose.onNodeWithText("Disconnected").assertDoesNotExist()
        compose.runOnIdle { error.value = SyncFailure("Network unavailable", true) }
        compose.onNodeWithText("Reconnect").assertIsDisplayed()
    }
}
