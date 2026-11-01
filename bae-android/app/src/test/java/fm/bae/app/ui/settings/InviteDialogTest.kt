package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.openTestLibrary
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.BaeTheme
import kotlinx.coroutines.awaitCancellation
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteAccess
import uniffi.bae_bridge.BridgeInviteDelivery
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeStorageLocation

/** Creating an invite gives the person access to storage over the network, so the dialog says so while it runs. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class InviteDialogTest {
    @get:Rule
    val compose = createComposeRule()

    /** A handle whose invite never finishes, so the dialog stays on its wait. */
    private class HeldInviteHandle : FakeAppHandle() {
        override suspend fun invitePerson(
            role: BridgeMemberRole,
            name: String,
            access: BridgeInviteAccess,
            delivery: BridgeInviteDelivery,
        ): BridgeInvite = awaitCancellation()
    }

    @Test
    fun aRunningInviteSaysItIsCreatingIt() {
        val session = openTestLibrary(HeldInviteHandle())
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    InviteDialog(
                        session = session,
                        storage = BridgeStorageLocation.Dropbox(BridgeDropboxLocation.AppFolder, accountLabel = "ana@example.com"),
                        onCreated = {},
                        onDismiss = {},
                    )
                }
            }
        }
        compose.onNodeWithText("Creating the invite…").assertDoesNotExist()

        compose.onNodeWithText("Name").performTextInput("Ben")
        compose.onNodeWithText("Email").performTextInput("ben@example.com")
        compose.onNodeWithText("Create invite").performClick()

        // The line ends the form, which scrolls inside the dialog.
        compose.onNodeWithText("Creating the invite…").performScrollTo().assertIsDisplayed()
    }
}
