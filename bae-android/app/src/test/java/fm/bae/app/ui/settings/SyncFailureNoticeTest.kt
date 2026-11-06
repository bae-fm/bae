package fm.bae.app.ui.settings

import android.app.Application
import android.content.ClipboardManager
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.data.SyncFailure
import fm.bae.app.ui.BaeTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SyncFailureNoticeTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aFailureShowsItsReasonAlone() {
        compose.setContent {
            BaeTheme {
                SyncFailureNotice(
                    error =
                        SyncFailure(
                            DisplayError("Update the app to continue syncing."),
                            offersReconnect = false,
                            offersDeviceReset = false,
                        ),
                    onReconnect = {},
                )
            }
        }

        compose.onNodeWithText("Update the app to continue syncing.").assertIsDisplayed()
    }

    /** A failure core reported shows its fault under the line, and Copy details copies the whole diagnostic. */
    @Test
    fun aFailureShowsItsCauseAndCopiesTheWholeDiagnostic() {
        val detail = "Network: S3 request failed: dispatch failure: io error: dns error: no such host\nsecond line"
        compose.setContent {
            BaeTheme {
                SyncFailureNotice(
                    error =
                        SyncFailure(
                            DisplayError("Couldn't reach the network.", detail),
                            offersReconnect = false,
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

    /** A start that failed offers Reconnect beside why, and pressing it reconnects; another failure offers none. */
    @Test
    fun aFailedStartOffersReconnect() {
        var offered by mutableStateOf(true)
        var reconnects = 0
        compose.setContent {
            BaeTheme {
                SyncFailureNotice(
                    error =
                        SyncFailure(
                            DisplayError("Sync couldn't start."),
                            offersReconnect = offered,
                            offersDeviceReset = false,
                        ),
                    onReconnect = { reconnects++ },
                )
            }
        }

        compose.onNodeWithText("Reconnect").performClick()
        assertEquals(1, reconnects)

        offered = false
        compose.onNodeWithText("Reconnect").assertDoesNotExist()
    }

    @Test
    fun withoutAFailureShowsNothing() {
        compose.setContent {
            BaeTheme { SyncFailureNotice(error = null, onReconnect = {}) }
        }

        compose.onNodeWithContentDescription("Copy details").assertDoesNotExist()
    }
}
